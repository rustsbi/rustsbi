use super::{
    Cause, Frame, HypervisorTrap, Trap,
    entry::HartTrapState,
    illegal::{DecodedTimeRead, TimeCsr},
};

#[test]
fn trap_exposes_only_decoded_observations() {
    let mut frame = Frame::test_frame(0x8020_0000, 5, 0x1234);
    let state = HartTrapState::new();
    let trap = Trap {
        frame: &mut frame,
        state: &state,
    };

    assert_eq!(trap.cause(), Cause::Other);
}

#[test]
fn interrupt_bit_is_removed_from_public_cause_number() {
    let encoded = (1usize << (usize::BITS - 1)) | 7;
    let mut frame = Frame::test_frame(0x1000, encoded, 0);
    let state = HartTrapState::new();
    let trap = Trap {
        frame: &mut frame,
        state: &state,
    };

    assert_eq!(trap.cause(), Cause::MachineTimerInterrupt);
}

#[test]
fn sbi_call_contains_only_copied_abi_values() {
    let mut frame = Frame::test_frame(0x8020_0000, 9, 0);
    frame.test_set_previous_mode(1);
    for (register, value) in (10..=17).zip(0x10..=0x17) {
        assert!(frame.set_register(register, value));
    }
    let state = HartTrapState::new();
    let trap = Trap {
        frame: &mut frame,
        state: &state,
    };

    assert_eq!(
        trap.cause(),
        Cause::SbiCall {
            extension_id: 0x17,
            function_id: 0x16,
            arguments: [0x10, 0x11, 0x12, 0x13, 0x14, 0x15],
        }
    );
}

#[test]
fn illegal_decoder_accepts_only_read_only_time_csrrs() {
    let csrrs = |csr: usize, rd: usize, rs1: usize| {
        (csr << 20) | (rs1 << 15) | (0b010 << 12) | (rd << 7) | 0x73
    };
    assert_eq!(
        super::decode_time_read(csrrs(0xc01, 5, 0)),
        Some(DecodedTimeRead {
            destination_register: 5,
            csr: TimeCsr::Time,
        })
    );
    assert_eq!(
        super::decode_time_read(csrrs(0xc81, 7, 0)),
        Some(DecodedTimeRead {
            destination_register: 7,
            csr: TimeCsr::TimeHigh,
        })
    );
    assert_eq!(super::decode_time_read(csrrs(0xc01, 5, 1)), None);
    assert_eq!(super::decode_time_read(csrrs(0xc00, 5, 0)), None);
    assert_eq!(super::decode_time_read(0), None);
    #[cfg(target_pointer_width = "64")]
    assert_eq!(super::decode_time_read(1usize << 32), None);
}

#[test]
fn hs_status_matches_virtual_and_nonvirtual_trap_entry_rules() {
    const GVA: usize = 1 << 6;
    const SPV: usize = 1 << 7;
    const SPVP: usize = 1 << 8;
    let metadata = |virtualized, previous_supervisor, guest_address| HypervisorTrap {
        virtualized,
        previous_supervisor,
        guest_address,
        value2: 0,
        instruction: 0,
    };

    let (nonvirtual, mask) =
        super::hypervisor_status(GVA | SPV | SPVP, metadata(false, false, false));
    assert_eq!(mask, GVA | SPV);
    assert_eq!(nonvirtual & (GVA | SPV), 0);
    assert_ne!(nonvirtual & SPVP, 0);

    let (virtual_user, mask) = super::hypervisor_status(SPVP, metadata(true, false, true));
    assert_eq!(mask, GVA | SPV | SPVP);
    assert_eq!(virtual_user & (GVA | SPV | SPVP), GVA | SPV);

    let (virtual_supervisor, _) = super::hypervisor_status(0, metadata(true, true, true));
    assert_eq!(virtual_supervisor & (GVA | SPV | SPVP), GVA | SPV | SPVP);
}
