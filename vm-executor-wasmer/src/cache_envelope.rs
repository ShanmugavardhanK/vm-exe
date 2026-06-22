use multiversx_chain_vm_executor::{CompilationOptionsLegacy, ExecutorError, ServiceError};

const CACHE_MAGIC: &[u8; 8] = b"MXVMCCH1";
const CACHE_ENVELOPE_VERSION: u32 = 1;
const HEADER_LEN: usize = 8 + 4 + 4 + 8 + 8 + 8;

const CACHE_ENVELOPE_TOO_SHORT: &str = "compiled cache envelope is too short";
const CACHE_ENVELOPE_BAD_MAGIC: &str = "compiled cache envelope magic mismatch";
const CACHE_ENVELOPE_BAD_VERSION: &str = "compiled cache envelope version mismatch";
const CACHE_ENVELOPE_BAD_LENGTH: &str = "compiled cache envelope payload length mismatch";
const CACHE_ENVELOPE_BAD_OPTIONS: &str = "compiled cache envelope compilation options mismatch";
const CACHE_ENVELOPE_BAD_HASH: &str = "compiled cache envelope hash mismatch";

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[cfg(test)]
pub(crate) fn encode_cache_artifact(
    artifact: &[u8],
    compilation_options: &CompilationOptionsLegacy,
) -> Vec<u8> {
    encode_cache_artifact_with_options_fingerprint(
        artifact,
        compilation_options_fingerprint(compilation_options),
    )
}

pub(crate) fn encode_cache_artifact_with_options_fingerprint(
    artifact: &[u8],
    options_fingerprint: u64,
) -> Vec<u8> {
    let mut envelope = Vec::with_capacity(HEADER_LEN + artifact.len());

    envelope.extend_from_slice(CACHE_MAGIC);
    envelope.extend_from_slice(&CACHE_ENVELOPE_VERSION.to_le_bytes());
    envelope.extend_from_slice(&0u32.to_le_bytes());
    envelope.extend_from_slice(&0u64.to_le_bytes());
    envelope.extend_from_slice(&options_fingerprint.to_le_bytes());
    envelope.extend_from_slice(&(artifact.len() as u64).to_le_bytes());
    envelope.extend_from_slice(artifact);

    let hash = cache_payload_hash(options_fingerprint, artifact);
    envelope[16..24].copy_from_slice(&hash.to_le_bytes());

    envelope
}

pub(crate) fn decode_cache_artifact<'a>(
    envelope: &'a [u8],
    compilation_options: &CompilationOptionsLegacy,
) -> Result<&'a [u8], ExecutorError> {
    if envelope.len() < HEADER_LEN {
        return Err(Box::new(ServiceError::new(CACHE_ENVELOPE_TOO_SHORT)));
    }
    if &envelope[0..8] != CACHE_MAGIC {
        return Err(Box::new(ServiceError::new(CACHE_ENVELOPE_BAD_MAGIC)));
    }

    let version = u32::from_le_bytes(envelope[8..12].try_into().expect("fixed-size version"));
    if version != CACHE_ENVELOPE_VERSION {
        return Err(Box::new(ServiceError::new(CACHE_ENVELOPE_BAD_VERSION)));
    }

    let expected_hash = u64::from_le_bytes(envelope[16..24].try_into().expect("fixed-size hash"));
    let encoded_options =
        u64::from_le_bytes(envelope[24..32].try_into().expect("fixed-size options"));
    let expected_payload_len =
        u64::from_le_bytes(envelope[32..40].try_into().expect("fixed-size length")) as usize;

    let payload = &envelope[HEADER_LEN..];
    if payload.len() != expected_payload_len {
        return Err(Box::new(ServiceError::new(CACHE_ENVELOPE_BAD_LENGTH)));
    }

    let actual_options = compilation_options_fingerprint(compilation_options);
    if encoded_options != actual_options {
        return Err(Box::new(ServiceError::new(CACHE_ENVELOPE_BAD_OPTIONS)));
    }

    let actual_hash = cache_payload_hash(encoded_options, payload);
    if expected_hash != actual_hash {
        return Err(Box::new(ServiceError::new(CACHE_ENVELOPE_BAD_HASH)));
    }

    Ok(payload)
}

fn cache_payload_hash(options_fingerprint: u64, artifact: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    hash_bytes(&mut hash, CACHE_MAGIC);
    hash_bytes(&mut hash, &CACHE_ENVELOPE_VERSION.to_le_bytes());
    hash_bytes(&mut hash, env!("CARGO_PKG_VERSION").as_bytes());
    hash_bytes(&mut hash, &options_fingerprint.to_le_bytes());
    hash_bytes(&mut hash, artifact);
    hash
}

pub(crate) fn compilation_options_fingerprint(options: &CompilationOptionsLegacy) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    hash_bytes(&mut hash, CACHE_MAGIC);
    hash_bytes(&mut hash, &CACHE_ENVELOPE_VERSION.to_le_bytes());
    hash_bytes(&mut hash, env!("CARGO_PKG_VERSION").as_bytes());
    hash_bytes(&mut hash, &(options.unmetered_locals as u64).to_le_bytes());
    hash_bytes(&mut hash, &(options.max_memory_grow as u64).to_le_bytes());
    hash_bytes(
        &mut hash,
        &(options.max_memory_grow_delta as u64).to_le_bytes(),
    );
    hash_bool(&mut hash, options.opcode_trace);
    hash_bool(&mut hash, options.metering);
    hash_bool(&mut hash, options.runtime_breakpoints);
    hash
}

fn hash_bool(hash: &mut u64, value: bool) {
    hash_bytes(hash, &[u8::from(value)]);
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> CompilationOptionsLegacy {
        CompilationOptionsLegacy {
            gas_limit: 1_000_000,
            unmetered_locals: 64,
            max_memory_grow: 10,
            max_memory_grow_delta: 2,
            opcode_trace: false,
            metering: true,
            runtime_breakpoints: true,
        }
    }

    #[test]
    fn cache_envelope_round_trips_artifact() {
        let artifact = [0xabu8; 32];
        let options = options();

        let envelope = encode_cache_artifact(&artifact, &options);
        let decoded = decode_cache_artifact(&envelope, &options).unwrap();

        assert_eq!(decoded, artifact);
    }

    #[test]
    fn cache_envelope_rejects_modified_payload() {
        let artifact = b"wasmer-cache-artifact";
        let options = options();
        let mut envelope = encode_cache_artifact(artifact, &options);
        let last = envelope.last_mut().unwrap();
        *last ^= 0xff;

        let err = decode_cache_artifact(&envelope, &options).unwrap_err();

        assert_eq!(err.to_string(), CACHE_ENVELOPE_BAD_HASH);
    }

    #[test]
    fn cache_envelope_rejects_different_compilation_options() {
        let artifact = b"wasmer-cache-artifact";
        let original_options = options();
        let mut different_options = options();
        different_options.metering = false;

        let envelope = encode_cache_artifact(artifact, &original_options);
        let err = decode_cache_artifact(&envelope, &different_options).unwrap_err();

        assert_eq!(err.to_string(), CACHE_ENVELOPE_BAD_OPTIONS);
    }

    #[test]
    fn cache_envelope_accepts_different_runtime_gas_limit() {
        let artifact = b"wasmer-cache-artifact";
        let original_options = options();
        let mut restore_options = options();
        restore_options.gas_limit = original_options.gas_limit + 1;

        let envelope = encode_cache_artifact(artifact, &original_options);
        let decoded = decode_cache_artifact(&envelope, &restore_options).unwrap();

        assert_eq!(decoded, artifact);
    }

    #[test]
    fn cache_envelope_rejects_raw_legacy_cache_bytes() {
        let options = options();
        let raw_cache = b"raw-wasmer-cache-without-envelope";

        let err = decode_cache_artifact(raw_cache, &options).unwrap_err();

        assert_eq!(err.to_string(), CACHE_ENVELOPE_TOO_SHORT);
    }
}
