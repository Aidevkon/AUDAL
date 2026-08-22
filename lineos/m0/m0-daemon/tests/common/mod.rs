//! Shared test-support helpers. `tests/common/` (not `tests/common.rs`)
//! so cargo does not compile it as its own test binary.

/// Rust mirror of scripts/verify_cert.py: find the ONE
/// `"payload_signature": "..."` anchor, zero the value back out of
/// the bytes, verify against `signer_public_key` read from the SAME
/// file. No struct re-serialize anywhere in this path.
///
/// Moved here from inv_sig_1_sidecar_signature.rs so a second
/// consumer (output_acx_delivered_certificate.rs) can import it
/// instead of copying it — same logic, one definition.
pub fn verify_sidecar_bytes(text: &str) -> Result<(), String> {
    let anchor_prefix = "\"payload_signature\": \"";
    let occurrences = text.matches(anchor_prefix).count();
    if occurrences != 1 {
        return Err(format!(
            "payload_signature anchor found {occurrences} times, expected exactly 1"
        ));
    }
    let start = text.find(anchor_prefix).unwrap() + anchor_prefix.len();
    let end = start
        + text[start..]
            .find('"')
            .ok_or("unterminated payload_signature value")?;
    let sig_b64 = &text[start..end];
    if sig_b64.is_empty() {
        return Err("payload_signature is empty — unsigned (pre-Σ1α) cert".into());
    }

    // Reconstruct the exact bytes that were signed: same anchor, value zeroed.
    let unsigned = format!("{}{}", &text[..start], &text[end..]);

    let envelope: serde_json::Value =
        serde_json::from_str(text).map_err(|e| e.to_string())?;
    let signer_hex = envelope["signer_public_key"]
        .as_str()
        .ok_or("missing signer_public_key")?;

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|e| e.to_string())?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "signature is not 64 bytes".to_string())?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

    let pubkey_bytes = hex::decode(signer_hex).map_err(|e| e.to_string())?;
    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| "public key is not 32 bytes".to_string())?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_array)
        .map_err(|e| e.to_string())?;

    use ed25519_dalek::Verifier;
    verifying_key
        .verify(unsigned.as_bytes(), &signature)
        .map_err(|e| e.to_string())
}
