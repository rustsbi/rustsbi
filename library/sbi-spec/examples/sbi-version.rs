//! This example illustrates how to use the `Version` structure which represents a valid
//! RISC-V SBI version.

/// Import the version type defined in the SBI specification, used for representing and
/// manipulating SBI version numbers.
use sbi_spec::base::Version;

/// We mock a S-mode software (kernel, etc.) that runs on minimum SBI version of v1.0;
/// it will detect from the environment and judge if it meets the minimum SBI version demand.
fn main() {
    // Create a version structure representing the minimum version at RISC-V SBI v1.0.
    let minimum_version = Version::V1_0;

    // Call the mock SBI runtime to obtain the current version number.
    println!("Probing SBI version of current environment...");
    let current_version = sbi::get_spec_version();

    // Print the detected SBI version.
    println!("Kernel running on SBI version {}", current_version);

    // Version comparison: Check whether the current version meets the minimum
    // requirement (v1.0 or higher).
    if current_version >= minimum_version {
        // The version meets the requirement, output success message.
        println!("The SBI version meets minimum demand of RISC-V SBI v1.0.");
        println!("✓ Test success!");
    } else {
        // The version is too low, output failure message.
        println!("✗ Test failed, SBI version is too low.");
    }
}

/* -- Implementation of a mock SBI runtime -- */

/// Module simulating an SBI runtime for the test environment.
mod sbi {
    use sbi_spec::base::Version;

    /// Mock function to retrieve the SBI specification version.
    pub fn get_spec_version() -> Version {
        // Return a hardcoded version number `0x0200_0000`.
        // Using the parsing rule from the RISC-V SBI Specification, this represents major
        // version 2, minor version 0 (i.e., 2.0).
        // In a real environment, this function should interact with the SBI implementation
        // via ECALL to obtain the actual version.
        Version::from_raw(0x0200_0000)
    }
}

/* Code execution result analysis:
   The current simulated SBI version is 2.0, which is higher than the minimum requirement
   of 1.0, so the output will be:
   ✓ Test success!

   To test a failure scenario, modify the mock return value to a version lower than 1.0,
   for example:
   `Version::from_raw(0x0000_0002)` represents 0.2.
*/
