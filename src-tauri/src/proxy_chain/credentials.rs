//! Current-user DPAPI for the first hop only. Never return stored secrets to UI.
use super::error::ChainError;
use base64::{engine::general_purpose::STANDARD, Engine as _};

fn failure() -> ChainError {
    ChainError::new("proxy_chain_credentials_unavailable", "configuration", "System proxy credentials could not be protected or read for this Windows account. Enter them again.")
}

#[cfg(windows)]
fn crypt(input: &[u8], protect: bool) -> Result<Vec<u8>, ChainError> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: input.len().try_into().map_err(|_| failure())?,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: input is live for the call; DPAPI owns output until LocalFree.
    unsafe {
        let ok = if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(failure());
        }
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as _);
        Ok(bytes)
    }
}

#[cfg(not(windows))]
fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>, ChainError> {
    Err(failure())
}

pub fn protect(password: &str) -> Result<String, ChainError> {
    crypt(password.as_bytes(), true).map(|b| STANDARD.encode(b))
}

pub fn unprotect(encoded: &str) -> Result<String, ChainError> {
    let bytes = STANDARD.decode(encoded).map_err(|_| failure())?;
    String::from_utf8(crypt(&bytes, false)?).map_err(|_| failure())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn current_user_round_trip_and_invalid_ciphertext() {
        let value = "synthetic-first-hop-only";
        let sealed = super::protect(value).unwrap();
        assert!(!sealed.contains(value));
        assert_eq!(super::unprotect(&sealed).unwrap(), value);
        assert!(super::unprotect("invalid").is_err());
    }
}
