use serde::Serialize;
use thiserror::Error;

const TARGET_NAME: &str = "com.yihui.notetaker.openai";
const USER_NAME: &str = "api-key";
const MAX_API_KEY_BYTES: usize = 2048;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiApiKeyStatus {
    pub has_key: bool,
    pub source: String,
}

#[derive(Debug, Error)]
pub enum OpenAiCredentialError {
    #[error("OpenAI API key is empty.")]
    EmptyKey,
    #[error("OpenAI API key is too long.")]
    KeyTooLong,
    #[cfg(target_os = "windows")]
    #[error("Windows Credential Manager error: {0}")]
    Windows(#[from] windows::core::Error),
    #[cfg(not(target_os = "windows"))]
    #[error("Credential storage is only implemented on Windows.")]
    UnsupportedPlatform,
}

pub fn get_status() -> Result<OpenAiApiKeyStatus, OpenAiCredentialError> {
    if std::env::var("OPENAI_API_KEY")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return Ok(OpenAiApiKeyStatus {
            has_key: true,
            source: "environment".to_string(),
        });
    }

    Ok(OpenAiApiKeyStatus {
        has_key: load_stored_api_key()?.is_some(),
        source: "credential-manager".to_string(),
    })
}

pub fn save_api_key(api_key: &str) -> Result<OpenAiApiKeyStatus, OpenAiCredentialError> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(OpenAiCredentialError::EmptyKey);
    }
    if trimmed.as_bytes().len() > MAX_API_KEY_BYTES {
        return Err(OpenAiCredentialError::KeyTooLong);
    }
    save_stored_api_key(trimmed)?;
    get_status()
}

pub fn clear_api_key() -> Result<OpenAiApiKeyStatus, OpenAiCredentialError> {
    delete_stored_api_key()?;
    get_status()
}

pub fn load_api_key() -> Result<Option<String>, OpenAiCredentialError> {
    if let Ok(value) = std::env::var("OPENAI_API_KEY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    load_stored_api_key()
}

#[cfg(target_os = "windows")]
fn save_stored_api_key(api_key: &str) -> Result<(), OpenAiCredentialError> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let mut target_name = to_wide_null(TARGET_NAME);
    let mut user_name = to_wide_null(USER_NAME);
    let mut blob = api_key.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target_name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user_name.as_mut_ptr()),
        ..Default::default()
    };

    unsafe { CredWriteW(&credential, 0)? };
    Ok(())
}

#[cfg(target_os = "windows")]
fn load_stored_api_key() -> Result<Option<String>, OpenAiCredentialError> {
    use std::ffi::c_void;
    use std::ptr;
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target_name = to_wide_null(TARGET_NAME);
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    let result = unsafe {
        CredReadW(
            PCWSTR(target_name.as_ptr()),
            CRED_TYPE_GENERIC,
            0,
            &mut credential,
        )
    };
    if result.is_err() || credential.is_null() {
        return Ok(None);
    }

    let credential_ref = unsafe { &*credential };
    let bytes = unsafe {
        std::slice::from_raw_parts(
            credential_ref.CredentialBlob,
            credential_ref.CredentialBlobSize as usize,
        )
    };
    let value = String::from_utf8_lossy(bytes).trim().to_string();
    unsafe { CredFree(credential as *const c_void) };

    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

#[cfg(target_os = "windows")]
fn delete_stored_api_key() -> Result<(), OpenAiCredentialError> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    if load_stored_api_key()?.is_none() {
        return Ok(());
    }

    let target_name = to_wide_null(TARGET_NAME);
    unsafe { CredDeleteW(PCWSTR(target_name.as_ptr()), CRED_TYPE_GENERIC, 0)? };
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn save_stored_api_key(_api_key: &str) -> Result<(), OpenAiCredentialError> {
    Err(OpenAiCredentialError::UnsupportedPlatform)
}

#[cfg(not(target_os = "windows"))]
fn load_stored_api_key() -> Result<Option<String>, OpenAiCredentialError> {
    Ok(None)
}

#[cfg(not(target_os = "windows"))]
fn delete_stored_api_key() -> Result<(), OpenAiCredentialError> {
    Err(OpenAiCredentialError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn to_wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
