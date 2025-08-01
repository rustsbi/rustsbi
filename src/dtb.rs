use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;
use core::ptr;
use core::str;

const FDT_BEGIN_NODE: u32 = 0x00000001;
const FDT_END_NODE: u32 = 0x00000002;
const FDT_PROP: u32 = 0x00000003;
const FDT_NOP: u32 = 0x00000004;
const FDT_END: u32 = 0x00000009;
const FDT_MAGIC: u32 = 0xd00dfeed;

pub static mut GLOBAL_NOW_DTB_ADDRESS: usize = 0;

struct DtbMemory<'a> {
    data: &'a mut [u8],
}

impl<'a> DtbMemory<'a> {
    fn new(data: &'a mut [u8]) -> Self {
        Self { data }
    }

    fn check_bounds(&self, offset: usize, len: usize) -> bool {
        offset
            .checked_add(len)
            .map_or(false, |end| end <= self.data.len())
    }

    fn read_u32(&self, offset: usize) -> Option<u32> {
        if !self.check_bounds(offset, 4) {
            error!(
                "read_u32 out of bounds: offset={}, len=4, size={} (It won't panic. Try adjusting and then retry)",
                offset,
                self.data.len()
            );
            return None;
        }
        let bytes: [u8; 4] = self.data[offset..offset + 4].try_into().ok()?;
        Some(u32::from_be_bytes(bytes))
    }

    fn write_u32(&mut self, offset: usize, value: u32) -> bool {
        if !self.check_bounds(offset, 4) {
            error!(
                "write_u32 out of bounds: offset={}, len=4, size={} (It won't panic. Try adjusting and then retry)",
                offset,
                self.data.len()
            );
            return false;
        }
        self.data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        true
    }

    fn read_bytes(&self, offset: usize, len: usize) -> Option<&[u8]> {
        if !self.check_bounds(offset, len) {
            error!(
                "read_bytes out of bounds: offset={}, len={}, size={} (It won't panic. Try adjusting and then retry)",
                offset,
                len,
                self.data.len()
            );
            return None;
        }
        Some(&self.data[offset..offset + len])
    }

    fn write_bytes(&mut self, offset: usize, data: &[u8]) -> bool {
        if !self.check_bounds(offset, data.len()) {
            error!(
                "write_bytes out of bounds: offset={}, len={}, size={} (It won't panic. Try adjusting and then retry)",
                offset,
                data.len(),
                self.data.len()
            );
            return false;
        }
        self.data[offset..offset + data.len()].copy_from_slice(data);
        true
    }

    fn read_str(&self, offset: usize) -> Option<&str> {
        let mut len = 0;
        while len < 256 {
            if !self.check_bounds(offset + len, 1) {
                error!(
                    "read_str out of bounds while searching for null terminator: offset={}, len=1, size={} (It won't panic. Try adjusting and then retry)",
                    offset + len,
                    self.data.len()
                );
                return None;
            }
            if self.data[offset + len] == 0 {
                break;
            }
            len += 1;
        }
        self.read_bytes(offset, len)
            .and_then(|bytes| str::from_utf8(bytes).ok())
    }

    pub fn get_addr(&self) -> usize {
        self.data.as_ptr() as usize
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct FdtHeader {
    magic: [u8; 4],
    totalsize: [u8; 4],
    off_dt_struct: [u8; 4],
    off_dt_strings: [u8; 4],
    off_mem_rsvmap: [u8; 4],
    version: [u8; 4],
    last_comp_version: [u8; 4],
    boot_cpuid_phys: [u8; 4],
    size_dt_strings: [u8; 4],
    size_dt_struct: [u8; 4],
}

impl FdtHeader {
    fn read(memory: &DtbMemory) -> Option<Self> {
        let bytes = memory.read_bytes(0, mem::size_of::<Self>())?;
        Some(unsafe { ptr::read_unaligned(bytes.as_ptr() as *const Self) })
    }

    fn write(&self, memory: &mut DtbMemory) -> bool {
        let bytes = unsafe {
            core::slice::from_raw_parts((self as *const Self) as *const u8, mem::size_of::<Self>())
        };
        memory.write_bytes(0, bytes)
    }

    fn magic(&self) -> u32 {
        u32::from_be_bytes(self.magic)
    }
    fn totalsize(&self) -> u32 {
        u32::from_be_bytes(self.totalsize)
    }
    fn off_dt_struct(&self) -> u32 {
        u32::from_be_bytes(self.off_dt_struct)
    }
    fn off_dt_strings(&self) -> u32 {
        u32::from_be_bytes(self.off_dt_strings)
    }
    fn size_dt_struct(&self) -> u32 {
        u32::from_be_bytes(self.size_dt_struct)
    }
    fn size_dt_strings(&self) -> u32 {
        u32::from_be_bytes(self.size_dt_strings)
    }
    fn off_mem_rsvmap_val(&self) -> u32 {
        u32::from_be_bytes(self.off_mem_rsvmap)
    }

    fn set_totalsize(&mut self, val: u32) {
        self.totalsize = val.to_be_bytes();
    }
    fn set_off_dt_struct(&mut self, val: u32) {
        self.off_dt_struct = val.to_be_bytes();
    }
    fn set_off_dt_strings(&mut self, val: u32) {
        self.off_dt_strings = val.to_be_bytes();
    }
    fn set_size_dt_struct(&mut self, val: u32) {
        self.size_dt_struct = val.to_be_bytes();
    }
    fn set_size_dt_strings(&mut self, val: u32) {
        self.size_dt_strings = val.to_be_bytes();
    }
}

struct NodeLocation {
    start_offset: usize,
    end_offset: usize,
}

pub struct DtbParser {
    dtb_data: Vec<u8>, // Owned copy of the DTB data
    header: FdtHeader,
    current_offset: usize, // Current offset in u32 words relative to `off_dt_struct`
}

impl DtbParser {
    /// Creates a new `DtbParser` instance by copying the DTB from physical memory.
    /// It determines the total size of the DTB by reading its header.
    ///
    /// # Arguments
    /// * `addr` - The virt address where the DTB is located.
    ///
    /// Returns `Some(DtbParser)` if the DTB is valid and copied, `None` otherwise.
    pub fn new(addr: usize) -> Option<Self> {
        // Step 1: Read just the header to determine the total size.
        let header_size = mem::size_of::<FdtHeader>();
        let mut temp_header_bytes = vec![0u8; header_size];

        unsafe {
            ptr::copy_nonoverlapping(
                addr as *const u8,
                temp_header_bytes.as_mut_ptr(),
                header_size,
            );
        }

        let temp_mem_for_header = DtbMemory::new(&mut temp_header_bytes[..]);
        let header = FdtHeader::read(&temp_mem_for_header)?;

        // Validate the magic number
        if header.magic() != FDT_MAGIC {
            error!("Invalid DTB magic: {:#x}", header.magic());
            return None;
        }

        let total_size = header.totalsize() as usize;
        if total_size < header_size {
            error!(
                "DTB total size ({}) is smaller than header size ({}).",
                total_size, header_size
            );
            return None;
        }

        // Allocate memory for the full DTB and copy it.
        let mut dtb_data = vec![0u8; total_size];
        unsafe {
            ptr::copy_nonoverlapping(addr as *const u8, dtb_data.as_mut_ptr(), total_size);
        }

        debug!(
            "DTB copied to owned memory. Total size: {} bytes",
            dtb_data.len()
        );

        Some(Self {
            dtb_data,
            header,
            current_offset: 0,
        })
    }

    /// Provides a mutable `DtbMemory` view of the internal DTB data.
    fn get_memory_view(&mut self) -> DtbMemory {
        DtbMemory::new(&mut self.dtb_data[..])
    }

    /// Provides a read-only `DtbMemory` view of the internal DTB data.
    fn get_memory_view_read_only(&self) -> DtbMemory {
        let const_ptr = self.dtb_data.as_ptr();
        let len = self.dtb_data.len();
        DtbMemory::new(unsafe { core::slice::from_raw_parts_mut(const_ptr as *mut u8, len) })
    }

    /// Save the entire DTB to mem. The parser should not be used after this func.
    ///
    /// Returns `addr(usize)` to indicate the memory address saved to.
    pub fn save_to_mem(&self) -> usize {
        let size = self.dtb_data.len();
        let start_addr = axalloc::global_allocator()
            .alloc_pages(size / 4096 + 1, 4096)
            .unwrap();
        unsafe {
            ptr::copy_nonoverlapping(self.dtb_data.as_ptr(), start_addr as *mut u8, size);
        }
        debug!("save dtb to mem, addr is {:#x}", start_addr);
        start_addr
    }

    /// Reads the next u32 token from the structure block and advances `current_offset`.
    unsafe fn next_token(&mut self) -> Option<u32> {
        let offset_in_bytes = self.header.off_dt_struct() as usize + self.current_offset * 4;
        self.get_memory_view_read_only()
            .read_u32(offset_in_bytes)
            .map(|val| {
                self.current_offset += 1;
                val
            })
    }

    /// Reads a null-terminated node name from the structure block and advances `current_offset`.
    unsafe fn read_node_name(&mut self) -> Option<String> {
        let base_offset_in_bytes = self.header.off_dt_struct() as usize;
        let current_byte_offset = base_offset_in_bytes + self.current_offset * 4;

        let mut name_bytes = Vec::new();
        let mut len = 0;
        let mem_view = self.get_memory_view_read_only();

        loop {
            let byte = match mem_view.read_bytes(current_byte_offset + len, 1) {
                Some(bytes) => bytes[0],
                None => {
                    error!(
                        "read_node_name: Out of bounds while reading name at offset {:#x}",
                        current_byte_offset + len
                    );
                    break;
                }
            };

            if byte == 0 {
                break;
            }

            name_bytes.push(byte);
            len += 1;

            if len >= 256 {
                error!(
                    "read_node_name: Node name exceeds 256 bytes limit at offset {:#x}",
                    current_byte_offset
                );
                break;
            }
        }

        let total_len = len + 1;
        let aligned_len = (total_len + 3) & !3;
        self.current_offset += aligned_len / 4;

        Some(String::from_utf8_lossy(&name_bytes).into_owned())
    }

    /// Skips the current node's content until its `FDT_END_NODE` token is found.
    unsafe fn skip_node(&mut self) -> usize {
        let mut depth = 1;

        while depth > 0 {
            match unsafe { self.next_token() } {
                Some(FDT_BEGIN_NODE) => {
                    let _ = unsafe { self.read_node_name() };
                    depth += 1;
                }
                Some(FDT_END_NODE) => depth -= 1,
                Some(FDT_PROP) => {
                    let len = unsafe { self.next_token().unwrap_or(0) } as usize;
                    let _ = unsafe { self.next_token() }; // nameoff
                    self.current_offset += (len + 3) / 4;
                }
                Some(FDT_NOP) => {}
                Some(FDT_END) => {
                    error!("Malformed DTB: Reached FDT_END while skipping node.");
                    break;
                }
                _ => {
                    error!(
                        "Malformed DTB: Unknown token {:#x} while skipping node.",
                        self.current_offset
                    );
                    break;
                }
            }
        }
        self.current_offset
    }

    /// Finds a node by its full path.
    fn find_node(&mut self, path: &str) -> Option<NodeLocation> {
        let target_path = path.trim_matches('/');

        let saved_offset = self.current_offset;
        self.current_offset = 0;

        let mut current_path_segments: Vec<String> = Vec::new();
        let mut depth = 0;
        let mut _node_start_offset: Option<usize> = None; // will get warning if without '_'

        while let Some(token) = unsafe { self.next_token() } {
            match token {
                FDT_BEGIN_NODE => {
                    let node_token_offset = self.current_offset - 1;
                    if let Some(name) = unsafe { self.read_node_name() } {
                        if depth == 0 && name.is_empty() {
                            // Root node
                        } else {
                            current_path_segments.push(name.clone());
                        }
                        depth += 1;

                        let current_full_path = if current_path_segments.is_empty() {
                            String::from("")
                        } else {
                            current_path_segments.join("/")
                        };

                        if current_full_path == target_path {
                            _node_start_offset = Some(node_token_offset);
                            let end_offset_after_skip = unsafe { self.skip_node() };
                            self.current_offset = saved_offset;
                            return Some(NodeLocation {
                                start_offset: _node_start_offset.unwrap(),
                                end_offset: end_offset_after_skip,
                            });
                        }
                    } else {
                        error!(
                            "find_node: Error reading node name at offset {:#x}",
                            self.current_offset * 4
                        );
                        self.current_offset = saved_offset;
                        return None;
                    }
                }
                FDT_END_NODE => {
                    depth -= 1;
                    if depth > 0 {
                        current_path_segments.pop();
                    } else if depth == 0 && !current_path_segments.is_empty() {
                        current_path_segments.clear();
                    }
                }
                FDT_PROP => {
                    if let Some(len) = unsafe { self.next_token() } {
                        if let Some(_nameoff) = unsafe { self.next_token() } {
                            self.current_offset += (len as usize + 3) / 4;
                        } else {
                            error!(
                                "find_node: Malformed DTB, missing nameoff for property at offset {:#x}",
                                self.current_offset * 4
                            );
                            self.current_offset = saved_offset;
                            return None;
                        }
                    } else {
                        error!(
                            "find_node: Malformed DTB, missing length for property at offset {:#x}",
                            self.current_offset * 4
                        );
                        self.current_offset = saved_offset;
                        return None;
                    }
                }
                FDT_NOP => {}
                FDT_END => break,
                _ => {
                    error!(
                        "find_node: Unknown token {:#x} at offset {:#x}",
                        token,
                        self.current_offset * 4
                    );
                    self.current_offset = saved_offset;
                    return None;
                }
            }
        }

        self.current_offset = saved_offset;
        None
    }

    /// Finds the insertion point for new data within a node.
    fn find_insertion_point_in_node(&mut self, node_path: &str) -> Option<usize> {
        let Some(location) = self.find_node(node_path) else {
            error!("Parent node '{}' not found for insertion.", node_path);
            return None;
        };

        let insertion_point_word_offset = location.end_offset - 1;
        let insertion_point_byte_offset =
            self.header.off_dt_struct() as usize + insertion_point_word_offset * 4;

        Some(insertion_point_byte_offset)
    }

    /// Finds or adds a string to the string table and returns its offset.
    fn get_string_offset(&mut self, s: &str) -> Option<u32> {
        let current_strings_offset = self.header.off_dt_strings() as usize;
        let current_strings_size = self.header.size_dt_strings() as usize;

        let mem_view_ro = self.get_memory_view_read_only();

        // Search for existing string
        let mut offset = 0;
        while offset < current_strings_size {
            if let Some(existing_str) = mem_view_ro.read_str(current_strings_offset + offset) {
                if existing_str == s {
                    return Some(offset as u32);
                }
                offset += existing_str.len() + 1; // Include null terminator
            } else {
                error!("get_string_offset: Malformed string table.");
                return None;
            }
        }

        // String not found, append it
        let string_to_add = s.as_bytes();
        let string_len_with_null = string_to_add.len() + 1;
        let aligned_string_len = (string_len_with_null + 3) & !3;

        let new_strings_size = current_strings_size + aligned_string_len;
        let total_size_increase = aligned_string_len;

        // Reallocate DTB to accommodate new string
        let old_total_size = self.header.totalsize() as usize;
        let new_total_size = old_total_size + total_size_increase;

        let mut new_dtb_data = vec![0u8; new_total_size];
        let mut new_mem_view = DtbMemory::new(&mut new_dtb_data[..]);

        // Copy everything before the string table
        new_mem_view.write_bytes(0, &self.dtb_data[0..current_strings_offset]);

        // Copy the old string table to its new offset
        let new_strings_start_byte_offset = current_strings_offset;
        new_mem_view.write_bytes(
            new_strings_start_byte_offset,
            &self.dtb_data[current_strings_offset..current_strings_offset + current_strings_size],
        );

        // Add the new string
        let new_string_offset_in_table = current_strings_size;
        new_mem_view.write_bytes(
            new_strings_start_byte_offset + new_string_offset_in_table,
            string_to_add,
        );
        new_mem_view.write_u32(
            new_strings_start_byte_offset + new_string_offset_in_table + string_to_add.len(),
            0,
        );

        // Update header
        let mut new_header = FdtHeader::read(&new_mem_view).unwrap();
        new_header.set_totalsize(new_total_size as u32);
        new_header.set_size_dt_strings(new_strings_size as u32);
        new_header.write(&mut new_mem_view);

        self.dtb_data = new_dtb_data;
        self.header = new_header;

        debug!(
            "Added string '{}' to string table. New string table size: {} bytes",
            s,
            self.header.size_dt_strings()
        );

        Some(new_string_offset_in_table as u32)
    }

    /// Adds a new node under a specified parent node.
    ///
    /// # Arguments
    /// * `parent_path` - The full path to the parent node (e.g., "/chosen").
    /// * `new_node_name` - The name of the new node (e.g., "my-new-device").
    ///
    /// Returns `true` on success, `false` on failure.
    pub fn add_node(&mut self, parent_path: &str, new_node_name: &str) -> bool {
        let Some(insertion_byte_offset) = self.find_insertion_point_in_node(parent_path) else {
            error!(
                "Could not find insertion point for node '{}' under parent '{}'.",
                new_node_name, parent_path
            );
            return false;
        };

        // Add node name to string table
        let Some(_nameoff) = self.get_string_offset(new_node_name) else {
            error!(
                "Failed to add node name '{}' to string table.",
                new_node_name
            );
            return false;
        };

        // Construct the new node data
        let node_name_bytes = new_node_name.as_bytes();
        let node_name_len_with_null = node_name_bytes.len() + 1;
        let aligned_node_name_len = (node_name_len_with_null + 3) & !3;

        let node_data_len = 4 + aligned_node_name_len + 4;

        let mut new_node_bytes = Vec::with_capacity(node_data_len);
        new_node_bytes.extend_from_slice(&FDT_BEGIN_NODE.to_be_bytes());
        new_node_bytes.extend_from_slice(node_name_bytes);
        new_node_bytes.push(0);
        while new_node_bytes.len() % 4 != 0 {
            new_node_bytes.push(0);
        }
        new_node_bytes.extend_from_slice(&FDT_END_NODE.to_be_bytes());

        let total_size_increase = new_node_bytes.len();
        let old_total_size = self.header.totalsize() as usize;
        let new_total_size = old_total_size + total_size_increase;

        let mut new_dtb_data = vec![0u8; new_total_size];
        let mut new_mem_view = DtbMemory::new(&mut new_dtb_data[..]);

        let old_off_dt_struct = self.header.off_dt_struct() as usize;
        let old_size_dt_struct = self.header.size_dt_struct() as usize;
        let old_off_dt_strings = self.header.off_dt_strings() as usize;
        let old_size_dt_strings = self.header.size_dt_strings() as usize;

        // 1. Copy data BEFORE insertion point (header, rsvmap, struct block up to insertion)
        new_mem_view.write_bytes(0, &self.dtb_data[0..insertion_byte_offset]);

        // 2. Insert new node data
        new_mem_view.write_bytes(insertion_byte_offset, &new_node_bytes);

        // 3. Copy data AFTER insertion point, adjusting offsets
        let old_struct_end_byte_offset = old_off_dt_struct + old_size_dt_struct;

        new_mem_view.write_bytes(
            insertion_byte_offset + total_size_increase,
            &self.dtb_data[insertion_byte_offset..old_struct_end_byte_offset],
        );

        // 4. Copy string table, adjusting its offset
        let new_strings_start_byte_offset = old_off_dt_strings + total_size_increase;
        new_mem_view.write_bytes(
            new_strings_start_byte_offset,
            &self.dtb_data[old_off_dt_strings..old_off_dt_strings + old_size_dt_strings],
        );

        // 5. Update the header in the new DTB data
        let mut new_header = FdtHeader::read(&new_mem_view).unwrap();
        new_header.set_totalsize(new_total_size as u32);
        new_header.set_size_dt_struct(old_size_dt_struct as u32 + total_size_increase as u32);
        new_header.set_off_dt_strings(new_strings_start_byte_offset as u32);
        new_header.write(&mut new_mem_view);

        // 6. Replace the old dtb_data with the new one
        self.dtb_data = new_dtb_data;
        self.header = new_header;

        debug!(
            "Added node '{}' under '{}'. New total size: {} bytes",
            new_node_name,
            parent_path,
            self.dtb_data.len()
        );

        true
    }

    /// Adds a new property to a specified node.
    ///
    /// # Arguments
    /// * `node_path` - The full path to the node where the property will be added.
    /// * `prop_name` - The name of the new property.
    /// * `prop_value` - The value of the new property (as a byte slice).
    ///
    /// Returns `true` on success, `false` on failure.
    pub fn add_property(&mut self, node_path: &str, prop_name: &str, prop_value: &[u8]) -> bool {
        let Some(node_location) = self.find_node(node_path) else {
            error!(
                "Node '{}' not found for adding property '{}'.",
                node_path, prop_name
            );
            return false;
        };

        let insertion_point_word_offset = node_location.end_offset - 1;
        let insertion_byte_offset =
            self.header.off_dt_struct() as usize + insertion_point_word_offset * 4;

        // Add property name to string table
        let Some(nameoff) = self.get_string_offset(prop_name) else {
            error!(
                "Failed to add property name '{}' to string table.",
                prop_name
            );
            return false;
        };

        // Construct the new property data
        let prop_value_len = prop_value.len();
        let aligned_prop_value_len = (prop_value_len + 3) & !3;

        let prop_data_len = 4 + 4 + 4 + aligned_prop_value_len;

        let mut new_prop_bytes = Vec::with_capacity(prop_data_len);
        new_prop_bytes.extend_from_slice(&FDT_PROP.to_be_bytes());
        new_prop_bytes.extend_from_slice(&(prop_value_len as u32).to_be_bytes());
        new_prop_bytes.extend_from_slice(&nameoff.to_be_bytes());
        new_prop_bytes.extend_from_slice(prop_value);
        while new_prop_bytes.len() % 4 != 0 {
            new_prop_bytes.push(0);
        }

        let total_size_increase = new_prop_bytes.len();
        let old_total_size = self.header.totalsize() as usize;
        let new_total_size = old_total_size + total_size_increase;

        let mut new_dtb_data = vec![0u8; new_total_size];
        let mut new_mem_view = DtbMemory::new(&mut new_dtb_data[..]);

        let old_off_dt_struct = self.header.off_dt_struct() as usize;
        let old_size_dt_struct = self.header.size_dt_struct() as usize;
        let old_off_dt_strings = self.header.off_dt_strings() as usize;
        let old_size_dt_strings = self.header.size_dt_strings() as usize;

        // 1. Copy data BEFORE insertion point (header, rsvmap, struct block up to insertion)
        new_mem_view.write_bytes(0, &self.dtb_data[0..insertion_byte_offset]);

        // 2. Insert new property data
        new_mem_view.write_bytes(insertion_byte_offset, &new_prop_bytes);

        // 3. Copy data AFTER insertion point, adjusting offsets
        let old_struct_end_byte_offset = old_off_dt_struct + old_size_dt_struct;

        new_mem_view.write_bytes(
            insertion_byte_offset + total_size_increase,
            &self.dtb_data[insertion_byte_offset..old_struct_end_byte_offset],
        );

        // 4. Copy string table, adjusting its offset
        let new_strings_start_byte_offset = old_off_dt_strings + total_size_increase;
        new_mem_view.write_bytes(
            new_strings_start_byte_offset,
            &self.dtb_data[old_off_dt_strings..old_off_dt_strings + old_size_dt_strings],
        );

        // 5. Update the header in the new DTB data
        let mut new_header = FdtHeader::read(&new_mem_view).unwrap();
        new_header.set_totalsize(new_total_size as u32);
        new_header.set_size_dt_struct(old_size_dt_struct as u32 + total_size_increase as u32);
        new_header.set_off_dt_strings(new_strings_start_byte_offset as u32);
        new_header.write(&mut new_mem_view);

        // 6. Replace the old dtb_data with the new one
        self.dtb_data = new_dtb_data;
        self.header = new_header;

        debug!(
            "Added property '{}' to node '{}'. New total size: {} bytes",
            prop_name,
            node_path,
            self.dtb_data.len()
        );

        true
    }

    /// Modifies the value of a property within a specified node.
    /// If the new value is longer than the existing allocated space,
    /// the entire DTB is reallocated and rebuilt in memory.
    ///
    /// # Arguments
    /// * `node_path` - The full path to the node (e.g., "/chosen").
    /// * `prop_name` - The name of the property to modify (e.g., "bootargs").
    /// * `new_value` - The new string value for the property.
    ///
    /// Returns `true` on success, `false` on failure.
    pub fn modify_property(&mut self, node_path: &str, prop_name: &str, new_value: &str) -> bool {
        let Some(location) = self.find_node(node_path) else {
            error!("Node not found for modification: {}", node_path);
            return false;
        };

        let saved_offset = self.current_offset;
        self.current_offset = location.start_offset;

        // Skip FDT_BEGIN_NODE token and the node name
        unsafe {
            self.next_token();
        }
        let _ = unsafe { self.read_node_name() };

        let mut prop_token_offset_words = 0;
        let mut prop_len_words = 0;
        let mut prop_nameoff_word = 0;
        let mut found = false;

        // Search for the property within the found node's range
        while self.current_offset < location.end_offset {
            if let Some(token) = unsafe { self.next_token() } {
                match token {
                    FDT_PROP => {
                        prop_token_offset_words = self.current_offset - 1; // FDT_PROP token itself
                        if let Some(len) = unsafe { self.next_token() } {
                            if let Some(nameoff) = unsafe { self.next_token() } {
                                let mem_view_ro = self.get_memory_view_read_only();
                                if let Some(name) = mem_view_ro.read_str(
                                    self.header.off_dt_strings() as usize + nameoff as usize,
                                ) {
                                    if name == prop_name {
                                        prop_len_words = (len as usize + 3) / 4;
                                        prop_nameoff_word = nameoff;
                                        found = true;
                                        break;
                                    }
                                }
                                self.current_offset += (len as usize + 3) / 4;
                            } else {
                                error!(
                                    "modify_property: Malformed DTB, missing nameoff for property at offset {:#x}",
                                    self.current_offset * 4
                                );
                                break;
                            }
                        } else {
                            error!(
                                "modify_property: Malformed DTB, missing length for property at offset {:#x}",
                                self.current_offset * 4
                            );
                            break;
                        }
                    }
                    FDT_BEGIN_NODE => {
                        let _ = unsafe { self.read_node_name() };
                        unsafe { self.skip_node() };
                    }
                    FDT_END_NODE => break,
                    FDT_NOP => {}
                    _ => {
                        error!(
                            "modify_property: Unknown token {:#x} in node while searching for property at offset {:#x}",
                            token,
                            self.current_offset * 4
                        );
                        break;
                    }
                }
            } else {
                error!(
                    "modify_property: Unexpected end of DTB structure while searching for property."
                );
                break;
            }
        }

        if !found {
            error!("Property '{}' not found in node '{}'", prop_name, node_path);
            self.current_offset = saved_offset;
            return false;
        }

        let new_value_bytes = new_value.as_bytes();
        let new_prop_len_bytes = new_value_bytes.len();

        // Extract header values before creating mutable `mem_view`
        let header_off_dt_struct = self.header.off_dt_struct() as usize;
        let header_size_dt_struct = self.header.size_dt_struct() as usize;
        let header_off_dt_strings = self.header.off_dt_strings() as usize;
        let header_size_dt_strings = self.header.size_dt_strings() as usize;
        let header_off_mem_rsvmap_val = self.header.off_mem_rsvmap_val() as usize;

        let old_prop_data_byte_offset = header_off_dt_struct + (prop_token_offset_words + 3) * 4;
        let old_prop_len_bytes = prop_len_words * 4;

        if new_prop_len_bytes <= old_prop_len_bytes {
            // Case 1: New value fits or is shorter (in-place modification)
            debug!("modify_property: New value fits or is shorter. Modifying in-place.");
            let mut mem_view = self.get_memory_view();
            let success = mem_view.write_bytes(old_prop_data_byte_offset, new_value_bytes);

            if success && new_prop_len_bytes < old_prop_len_bytes {
                let padding_len = old_prop_len_bytes - new_prop_len_bytes;
                let padding = vec![0u8; padding_len];
                mem_view.write_bytes(old_prop_data_byte_offset + new_prop_len_bytes, &padding);
            }

            let prop_len_offset = header_off_dt_struct + (prop_token_offset_words + 1) * 4;
            mem_view.write_u32(prop_len_offset, new_prop_len_bytes as u32);

            self.current_offset = saved_offset;
            return success;
        } else {
            // Case 2: New value is longer (reallocate and rebuild DTB)
            debug!("modify_property: New value is longer. Reallocating and rebuilding DTB.");

            let size_increase = new_prop_len_bytes - old_prop_len_bytes;
            let old_total_size = self.header.totalsize() as usize;
            let new_total_size = old_total_size + size_increase;

            let mut new_dtb_data = vec![0u8; new_total_size];
            let mut new_mem_view = DtbMemory::new(&mut new_dtb_data[..]);

            let old_header_size = mem::size_of::<FdtHeader>();

            // 1. Copy header and reserved memory map
            new_mem_view.write_bytes(0, &self.dtb_data[0..old_header_size]);
            new_mem_view.write_bytes(
                header_off_mem_rsvmap_val,
                &self.dtb_data[header_off_mem_rsvmap_val..header_off_dt_struct],
            );

            // 2. Copy structure block BEFORE the modified property
            let struct_start_byte_offset = header_off_dt_struct;
            new_mem_view.write_bytes(
                struct_start_byte_offset,
                &self.dtb_data[struct_start_byte_offset..old_prop_data_byte_offset],
            );

            // 3. Write the new property (FDT_PROP, new_len, nameoff, new_value)
            let current_write_offset = old_prop_data_byte_offset;
            new_mem_view.write_u32(current_write_offset - 12, FDT_PROP); // FDT_PROP token
            new_mem_view.write_u32(current_write_offset - 8, new_prop_len_bytes as u32); // New length
            new_mem_view.write_u32(current_write_offset - 4, prop_nameoff_word); // Original nameoff
            new_mem_view.write_bytes(current_write_offset, new_value_bytes); // New value

            // 4. Copy structure block AFTER the modified property, adjusting offsets
            let old_struct_end_byte_offset = header_off_dt_struct + header_size_dt_struct;
            let old_data_after_prop_start = old_prop_data_byte_offset + old_prop_len_bytes;
            let new_data_after_prop_start = old_prop_data_byte_offset + new_prop_len_bytes;

            new_mem_view.write_bytes(
                new_data_after_prop_start,
                &self.dtb_data[old_data_after_prop_start..old_struct_end_byte_offset],
            );

            // 5. Copy the string table (it's usually at the end and doesn't shift relative to itself, but its *offset* from the start of the DTB will change if the struct block grows)
            let old_strings_start_byte_offset = header_off_dt_strings;
            let old_strings_size = header_size_dt_strings;
            let new_strings_start_byte_offset = old_strings_start_byte_offset + size_increase;
            new_mem_view.write_bytes(
                new_strings_start_byte_offset,
                &self.dtb_data[old_strings_start_byte_offset
                    ..old_strings_start_byte_offset + old_strings_size],
            );

            // 6. Update the header in the new DTB data
            let mut new_header = FdtHeader::read(&new_mem_view).unwrap();
            new_header.set_totalsize(new_total_size as u32);
            new_header.set_size_dt_struct(header_size_dt_struct as u32 + size_increase as u32);
            new_header.set_off_dt_strings(new_strings_start_byte_offset as u32);
            new_header.set_size_dt_strings(old_strings_size as u32);

            new_header.write(&mut new_mem_view);

            // 7. Replace the old dtb_data with the new one
            self.dtb_data = new_dtb_data;
            self.header = new_header;

            debug!(
                "DTB reallocated and modified successfully. New total size: {} bytes",
                self.dtb_data.len()
            );
            self.current_offset = saved_offset;
            return true;
        }
    }

    /// Parses and prints the entire DTB structure in a human-readable format.
    pub fn dump_all(&mut self) {
        self.current_offset = 0;
        let mut path_stack: Vec<String> = Vec::new();
        let mut depth = 0;

        axlog::ax_println!("\n--- DTB Parsing Output ---");

        while let Some(token) = unsafe { self.next_token() } {
            match token {
                FDT_BEGIN_NODE => {
                    if let Some(name) = unsafe { self.read_node_name() } {
                        if depth == 0 && name.is_empty() {
                            axlog::ax_println!("/ (root)");
                        } else {
                            path_stack.push(name.clone());
                            let current_path = alloc::format!("/{}", path_stack.join("/"));
                            axlog::ax_println!(
                                "{}{} (path: {})",
                                "  ".repeat(depth),
                                name,
                                current_path
                            );
                        }
                        depth += 1;
                    } else {
                        error!(
                            "dump_all: Error reading node name at offset {:#x}",
                            self.current_offset * 4
                        );
                        break;
                    }
                }
                FDT_END_NODE => {
                    depth -= 1;
                    if depth > 0 {
                        path_stack.pop();
                    }
                }
                FDT_PROP => {
                    if let Some(len) = unsafe { self.next_token() } {
                        if let Some(nameoff) = unsafe { self.next_token() } {
                            let mem_view_ro_local = self.get_memory_view_read_only();
                            let name = mem_view_ro_local
                                .read_str(self.header.off_dt_strings() as usize + nameoff as usize)
                                .unwrap_or("<invalid>");

                            let offset_in_bytes =
                                self.header.off_dt_struct() as usize + self.current_offset * 4;
                            let data = mem_view_ro_local
                                .read_bytes(offset_in_bytes, len as usize)
                                .unwrap_or(&[]);

                            let mut indent = String::new();
                            for _ in 0..depth {
                                indent.push_str("  ");
                            }

                            axlog::ax_print!("{}{} = ", indent, name);
                            self.print_property_value(data);
                            axlog::ax_println!("");

                            self.current_offset += (len as usize + 3) / 4;
                        } else {
                            error!(
                                "dump_all: Malformed DTB, missing nameoff for property at offset {:#x}",
                                self.current_offset * 4
                            );
                            break;
                        }
                    } else {
                        error!(
                            "dump_all: Malformed DTB, missing length for property at offset {:#x}",
                            self.current_offset * 4
                        );
                        break;
                    }
                }
                FDT_END => {
                    axlog::ax_println!("--- End of DTB Structure ---");
                    break;
                }
                FDT_NOP => {}
                _ => {
                    error!(
                        "dump_all: Unknown token {:#x} at offset {:#x}",
                        token,
                        self.current_offset * 4
                    );
                    break;
                }
            }
        }
    }

    /// Helper function to print property values.
    fn print_property_value(&self, data: &[u8]) {
        if data.is_empty() {
            axlog::ax_print!("[]");
            return;
        }

        if let Ok(s) = core::str::from_utf8(data) {
            if s.chars()
                .all(|c| c.is_ascii() && (c.is_ascii_graphic() || c == ' ' || c == '\0'))
            {
                axlog::ax_print!("\"{}\"", s.trim_end_matches('\0'));
                return;
            }
        }

        axlog::ax_print!("[");
        for (i, &byte) in data.iter().enumerate() {
            if i > 0 {
                axlog::ax_print!(" ");
            }
            axlog::ax_print!("{:02x}", byte);
        }
        axlog::ax_print!("]");

        if data.len() % 4 == 0 {
            axlog::ax_print!(" (u32: [");
            for chunk in data.chunks_exact(4) {
                let val = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                axlog::ax_print!("{:#x} ", val);
            }
            axlog::ax_print!("])");
        }
    }

    /// Reads the value of a specific property within a given node and prints it.
    ///
    /// # Arguments
    /// * `node_path` - The full path to the node (e.g., "/chosen").
    /// * `prop_name` - The name of the property to read (e.g., "bootargs").
    ///
    /// Returns `true` if the property is found and printed, `false` otherwise.
    pub fn read_property_value(&mut self, node_path: &str, prop_name: &str) -> bool {
        let Some(location) = self.find_node(node_path) else {
            error!("read_property_value: Node not found: {}", node_path);
            return false;
        };

        let saved_offset = self.current_offset;
        self.current_offset = location.start_offset;

        unsafe {
            self.next_token();
        }
        let _ = unsafe { self.read_node_name() };

        let mut found = false;

        while self.current_offset < location.end_offset {
            if let Some(token) = unsafe { self.next_token() } {
                match token {
                    FDT_PROP => {
                        if let Some(len) = unsafe { self.next_token() } {
                            if let Some(nameoff) = unsafe { self.next_token() } {
                                let mem_view_ro = self.get_memory_view_read_only();
                                if let Some(name) = mem_view_ro.read_str(
                                    self.header.off_dt_strings() as usize + nameoff as usize,
                                ) {
                                    if name == prop_name {
                                        let offset_in_bytes = self.header.off_dt_struct() as usize
                                            + self.current_offset * 4;
                                        let data = mem_view_ro
                                            .read_bytes(offset_in_bytes, len as usize)
                                            .unwrap_or(&[]);

                                        axlog::ax_print!("Value of {}:{} = ", node_path, prop_name);
                                        self.print_property_value(data);
                                        axlog::ax_println!("");
                                        found = true;
                                        break;
                                    }
                                }
                                self.current_offset += (len as usize + 3) / 4;
                            } else {
                                error!(
                                    "read_property_value: Malformed DTB, missing nameoff for property at offset {:#x}",
                                    self.current_offset * 4
                                );
                                break;
                            }
                        } else {
                            error!(
                                "read_property_value: Malformed DTB, missing length for property at offset {:#x}",
                                self.current_offset * 4
                            );
                            break;
                        }
                    }
                    FDT_BEGIN_NODE => {
                        let _ = unsafe { self.read_node_name() };
                        unsafe { self.skip_node() };
                    }
                    FDT_END_NODE => break,
                    FDT_NOP => {}
                    _ => {
                        error!(
                            "read_property_value: Unknown token {:#x} in node while searching for property at offset {:#x}",
                            token,
                            self.current_offset * 4
                        );
                        break;
                    }
                }
            } else {
                error!(
                    "read_property_value: Unexpected end of DTB structure while searching for property."
                );
                break;
            }
        }

        if !found {
            error!("Property '{}' not found in node '{}'", prop_name, node_path);
        }

        self.current_offset = saved_offset;
        found
    }
}
