use super::format::{
    END, END_NODE, HEADER_SIZE, LAST_COMPATIBLE_VERSION_OFFSET, MAGIC, RESERVATION_ENTRY_SIZE,
    RESERVATIONS_OFFSET_OFFSET, STRINGS_OFFSET_OFFSET, STRINGS_SIZE_OFFSET,
    STRUCTURE_OFFSET_OFFSET, STRUCTURE_SIZE_OFFSET, SUPPORTED_VERSION, TOTAL_SIZE_OFFSET,
    VERSION_OFFSET, read_usize,
};
use super::plan::MAX_FDT_DEPTH;
use super::writer::{
    append_string, pad_to, push_begin_node, push_cells, push_property, push_u32, write_header_field,
};
use super::{Error, Reservation, prepare_next_stage};
use alloc::{vec, vec::Vec};
use fdt::Fdt;

#[test]
fn adds_reserved_memory_using_root_cell_widths() {
    let source = fixture(1, 1, false);
    let reservation = range(0x1000, 0x2000);
    let output = prepare_next_stage(&source, Some(reservation), &[]).unwrap();
    let fdt = Fdt::new(&output).unwrap();
    let parent = fdt.find_node("/reserved-memory").unwrap();
    assert_eq!(property_u32(parent, "#address-cells"), 1);
    assert_eq!(property_u32(parent, "#size-cells"), 1);
    assert!(parent.property("ranges").is_some());
    let node = fdt.find_node("/reserved-memory/mmode_resv1@1000").unwrap();
    let region = node.reg().unwrap().next().unwrap();
    assert_eq!(region.starting_address as usize, 0x1000);
    assert_eq!(region.size, Some(0x2000));
    assert!(node.property("no-map").is_some());
}

#[test]
fn adds_child_to_existing_reserved_memory() {
    let source = fixture(2, 2, true);
    let output = prepare_next_stage(&source, Some(range(0x1_0000_0000, 0x4000)), &[]).unwrap();
    let fdt = Fdt::new(&output).unwrap();
    assert!(fdt.find_node("/reserved-memory/keep@2000").is_some());
    let node = fdt
        .find_node("/reserved-memory/mmode_resv1@100000000")
        .unwrap();
    let region = node.reg().unwrap().next().unwrap();
    assert_eq!(region.starting_address as usize, 0x1_0000_0000);
    assert_eq!(region.size, Some(0x4000));
}

#[test]
fn rejects_an_already_generated_firmware_reservation() {
    let source = fixture(2, 2, false);
    let reservation = range(0x1_0000_0000, 0x4000);
    let output = prepare_next_stage(&source, Some(reservation), &[]).unwrap();
    assert_eq!(
        prepare_next_stage(&output, Some(reservation), &[]),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn replaces_only_the_node_at_the_exact_path() {
    let source = fixture(2, 2, false);
    let output = prepare_next_stage(&source, None, &["/soc/clint@2000000"]).unwrap();
    let fdt = Fdt::new(&output).unwrap();
    assert!(fdt.find_node("/soc").is_some());
    assert!(fdt.find_node("/soc/clint@2000000").is_none());
    assert!(fdt.find_node("/other-bus/clint@2000000").is_some());
}

#[test]
fn rejects_relative_hidden_node_path() {
    let source = fixture(2, 2, false);
    assert_eq!(
        prepare_next_stage(&source, None, &["clint@2000000"]),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn adds_reservation_and_replaces_node_in_one_rewrite() {
    let source = fixture(2, 2, false);
    let output = prepare_next_stage(
        &source,
        Some(range(0x1_0000_0000, 0x4000)),
        &["/soc/clint@2000000"],
    )
    .unwrap();
    let fdt = Fdt::new(&output).unwrap();
    assert!(
        fdt.find_node("/reserved-memory/mmode_resv1@100000000")
            .is_some()
    );
    assert!(fdt.find_node("/soc/clint@2000000").is_none());
}

#[test]
fn rejects_value_that_does_not_fit_one_cell() {
    let source = fixture(1, 1, false);
    assert_eq!(
        prepare_next_stage(&source, Some(range(0x1_0000_0000, 0x1000)), &[]),
        Err(Error::Overflow)
    );
}

#[test]
fn rejects_malformed_structure_without_panicking() {
    let mut source = fixture(2, 2, false);
    let structure_offset = read_usize(&source, STRUCTURE_OFFSET_OFFSET).unwrap();
    source[structure_offset..structure_offset + 4].copy_from_slice(&0xffff_ffffu32.to_be_bytes());
    assert_eq!(
        prepare_next_stage(&source, None, &[]),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn rejects_unsupported_header_version() {
    let mut source = fixture(2, 2, false);
    write_header_field(
        &mut source,
        VERSION_OFFSET,
        (SUPPORTED_VERSION - 1) as usize,
    )
    .unwrap();
    assert_eq!(
        prepare_next_stage(&source, None, &[]),
        Err(Error::InvalidArgs)
    );

    write_header_field(&mut source, VERSION_OFFSET, SUPPORTED_VERSION as usize).unwrap();
    write_header_field(
        &mut source,
        LAST_COMPATIBLE_VERSION_OFFSET,
        (SUPPORTED_VERSION + 1) as usize,
    )
    .unwrap();
    assert_eq!(
        prepare_next_stage(&source, None, &[]),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn rejects_short_cell_width_property() {
    let mut strings = Vec::new();
    let address_cells = append_string(&mut strings, "#address-cells").unwrap();
    let mut structure = Vec::new();
    push_begin_node(&mut structure, "").unwrap();
    push_property(&mut structure, address_cells, &[0, 0]).unwrap();
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END);

    assert_eq!(
        super::plan::validate_structure(&structure, &strings),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn rejects_incomplete_reg_tuple() {
    let mut strings = Vec::new();
    let reg = append_string(&mut strings, "reg").unwrap();
    let mut structure = Vec::new();
    push_begin_node(&mut structure, "").unwrap();
    push_begin_node(&mut structure, "memory@0").unwrap();
    push_property(&mut structure, reg, &[0; 4]).unwrap();
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END);

    assert_eq!(
        super::plan::validate_structure(&structure, &strings),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn rejects_properties_after_children() {
    let mut strings = Vec::new();
    let model = append_string(&mut strings, "model").unwrap();
    let mut structure = Vec::new();
    push_begin_node(&mut structure, "").unwrap();
    push_begin_node(&mut structure, "child").unwrap();
    push_u32(&mut structure, END_NODE);
    push_property(&mut structure, model, b"invalid\0").unwrap();
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END);

    assert_eq!(
        super::plan::validate_structure(&structure, &strings),
        Err(Error::InvalidArgs)
    );
}

#[test]
fn rejects_nesting_beyond_the_fdt_parser_limit() {
    let mut structure = Vec::new();
    for index in 0..=MAX_FDT_DEPTH {
        push_begin_node(&mut structure, if index == 0 { "" } else { "node" }).unwrap();
    }

    assert_eq!(
        super::plan::validate_structure(&structure, &[]),
        Err(Error::InvalidArgs)
    );
}

fn range(start: usize, length: usize) -> Reservation {
    Reservation::new(start as u64, length as u64).unwrap()
}

fn property_u32(node: fdt::node::FdtNode<'_, '_>, name: &str) -> u32 {
    let value: [u8; 4] = node.property(name).unwrap().value.try_into().unwrap();
    u32::from_be_bytes(value)
}

fn fixture(address_cells: u32, size_cells: u32, existing_reserved: bool) -> Vec<u8> {
    let mut strings = Vec::new();
    let address_name = append_string(&mut strings, "#address-cells").unwrap();
    let size_name = append_string(&mut strings, "#size-cells").unwrap();
    let ranges_name = append_string(&mut strings, "ranges").unwrap();
    let reg_name = append_string(&mut strings, "reg").unwrap();

    let mut structure = Vec::new();
    push_begin_node(&mut structure, "").unwrap();
    push_property(&mut structure, address_name, &address_cells.to_be_bytes()).unwrap();
    push_property(&mut structure, size_name, &size_cells.to_be_bytes()).unwrap();
    if existing_reserved {
        push_begin_node(&mut structure, "reserved-memory").unwrap();
        push_property(&mut structure, address_name, &address_cells.to_be_bytes()).unwrap();
        push_property(&mut structure, size_name, &size_cells.to_be_bytes()).unwrap();
        push_property(&mut structure, ranges_name, &[]).unwrap();
        push_begin_node(&mut structure, "keep@2000").unwrap();
        let mut reg = Vec::new();
        push_cells(&mut reg, 0x2000, address_cells).unwrap();
        push_cells(&mut reg, 0x1000, size_cells).unwrap();
        push_property(&mut structure, reg_name, &reg).unwrap();
        push_u32(&mut structure, END_NODE);
        push_u32(&mut structure, END_NODE);
    }
    push_begin_node(&mut structure, "soc").unwrap();
    push_begin_node(&mut structure, "clint@2000000").unwrap();
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END_NODE);
    push_begin_node(&mut structure, "other-bus").unwrap();
    push_begin_node(&mut structure, "clint@2000000").unwrap();
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END_NODE);
    push_u32(&mut structure, END);

    let mut output = vec![0; HEADER_SIZE];
    write_header_field(&mut output, 0, MAGIC as usize).unwrap();
    write_header_field(&mut output, VERSION_OFFSET, SUPPORTED_VERSION as usize).unwrap();
    write_header_field(
        &mut output,
        LAST_COMPATIBLE_VERSION_OFFSET,
        (SUPPORTED_VERSION - 1) as usize,
    )
    .unwrap();
    let reservations_offset = output.len();
    output.extend_from_slice(&[0; RESERVATION_ENTRY_SIZE]);
    let structure_offset = output.len();
    output.extend_from_slice(&structure);
    let strings_offset = output.len();
    output.extend_from_slice(&strings);
    pad_to(&mut output, 4);
    let total_size = output.len();
    write_header_field(&mut output, TOTAL_SIZE_OFFSET, total_size).unwrap();
    write_header_field(&mut output, STRUCTURE_OFFSET_OFFSET, structure_offset).unwrap();
    write_header_field(&mut output, STRINGS_OFFSET_OFFSET, strings_offset).unwrap();
    write_header_field(&mut output, RESERVATIONS_OFFSET_OFFSET, reservations_offset).unwrap();
    write_header_field(&mut output, STRINGS_SIZE_OFFSET, strings.len()).unwrap();
    write_header_field(&mut output, STRUCTURE_SIZE_OFFSET, structure.len()).unwrap();
    output
}
