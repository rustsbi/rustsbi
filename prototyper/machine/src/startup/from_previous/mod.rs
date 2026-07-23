//! Raw entry mechanisms for information supplied by the previous stage.

mod fixed;
mod fw_dynamic;

/// Enters the raw mechanism named by the linked firmware contract.
///
/// # Safety
///
/// The previous stage must supply the register envelope required by the
/// selected standard firmware type.
#[unsafe(naked)]
#[unsafe(export_name = "__rustsbi_prototyper_from_previous")]
pub unsafe extern "C" fn raw_entry() -> ! {
    core::arch::naked_asm!(
        "lla t0, __prototyper_contract_start",
        "lbu t0, 6(t0)",
        "li t1, 1",
        "beq t0, t1, 1f",
        "li t1, 2",
        "beq t0, t1, 2f",
        "li t1, 3",
        "beq t0, t1, 2f",
        "3:",
        "wfi",
        "j 3b",
        "1:",
        "j {dynamic}",
        "2:",
        "j {fixed}",
        dynamic = sym fw_dynamic::entry,
        fixed = sym fixed::entry,
    )
}
