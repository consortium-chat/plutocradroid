pub const CSRF_COOKIE_NAME:&str = "csrf_protection_token_v2";

pub fn generate_state<A: rand::RngCore + rand::CryptoRng>(rng: &mut A) -> Result<String, &'static str> {
    let mut buf = [0; 16]; // 128 bits
    rng.try_fill_bytes(&mut buf).map_err(|_| "Failed to generate random data")?;
    Ok(base64::encode_config(&buf, base64::URL_SAFE_NO_PAD))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CSRFToken(pub String);

#[derive(Debug, Clone, FromForm)]
pub struct CSRFForm {
    pub csrf: String
}