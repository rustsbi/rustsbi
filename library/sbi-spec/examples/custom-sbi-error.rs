/// This example demonstrates how to define custom error types and map SBI error codes.
///
/// The code shows:
/// - How to create a custom error enum that wraps standard SBI errors and adds custom error codes
/// - Conversion logic between standard SBI errors and custom error types
/// - Error handling patterns using Rust's Result type and error matching
use sbi_spec::binary::{Error, SbiRet};

/// Custom error type for SBI-related operations
///
/// Contains three possible variants:
/// - Standard: Wraps standard SBI errors from the sbi_spec crate
/// - MyErrorCode1/MyErrorCode2: Custom error codes specific to this implementation
pub enum MyError {
    /// Wrapper for standard SBI errors defined in sbi_spec
    Standard(sbi_spec::binary::Error),
    /// First custom error code (maps to value 1001)
    MyErrorCode1,
    /// Second custom error code (maps to value 1002)
    MyErrorCode2,
}

/// Numeric value for first custom error code
const MY_ERROR_CODE_1: usize = 1001;
/// Numeric value for second custom error code
const MY_ERROR_CODE_2: usize = 1002;

/// Conversion implementation from standard SBI errors to our custom error type
///
/// This allows automatic conversion when using the ? operator in functions returning MyError.
/// The conversion logic:
/// - Maps specific custom error codes to their corresponding MyError variants
/// - Wraps all other standard errors in the Standard variant
impl From<sbi_spec::binary::Error> for MyError {
    fn from(value: Error) -> Self {
        match value {
            // Handle custom error code mappings
            Error::Custom(MY_ERROR_CODE_1) => MyError::MyErrorCode1,
            Error::Custom(MY_ERROR_CODE_2) => MyError::MyErrorCode2,
            // Wrap all other standard errors
            _ => MyError::Standard(value),
        }
    }
}

/// Main function demonstrating error handling workflow
///
/// The execution flow:
/// 1. Creates an SbiRet structure with success status
/// 2. Manually sets an error code for demonstration
/// 3. Converts the SbiRet to Result type with custom error mapping
/// 4. Pattern matches on the result to handle different error cases
fn main() {
    // Create a base SbiRet with success status (value = 0)
    let mut ret = SbiRet::success(0);
    // Override the error code for demonstration purposes
    ret.error = 1001;

    // Convert SbiRet to Result, mapping error codes to MyError variants
    let ans = ret.map_err(MyError::from);

    // Handle different error cases through pattern matching
    match ans {
        Ok(_) => println!("Okay"),
        // Match specific custom error codes
        Err(MyError::MyErrorCode1) => println!("Custom error code 1"),
        Err(MyError::MyErrorCode2) => println!("Custom error code 2"),
        // Handle wrapped standard SBI errors
        Err(MyError::Standard(err)) => println!("Standard error: {:?}", err),
    }
}
