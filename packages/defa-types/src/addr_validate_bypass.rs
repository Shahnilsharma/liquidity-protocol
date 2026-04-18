#[cfg(feature = "test-addr-bypass")]
pub fn addr_validate_bypass(addr: &str) -> Result<String, String> {
    // Accept any string as a valid address in test mode
    Ok(addr.to_string())
}

#[cfg(not(feature = "test-addr-bypass"))]
pub fn addr_validate_bypass(addr: &str) -> Result<String, String> {
    // This should never be called in production, always use deps.api.addr_validate
    Err("addr_validate_bypass called without test-addr-bypass feature".to_string())
}
