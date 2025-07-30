type CmdHandler = fn(&str);

const CMD_TABLE: &[(&str, &str, CmdHandler)] = &[
    ("help", "-- List the help for ArceBoot", do_help),
    ("exit", "-- Exit ArceBoot", do_exit),
    ("env", "-- List the envs", do_env),
    ("setenv", "-- Set the envs(Test now)", do_set_env),
];

pub fn run_cmd(line: &[u8]) {
    let line_str = unsafe { core::str::from_utf8_unchecked(line) };
    let (cmd, args) = split_whitespace(line_str);
    if !cmd.is_empty() {
        for (name, _, func) in CMD_TABLE {
            if cmd == *name {
                func(args);
                return;
            }
        }
        axlog::ax_println!("{}: command not found", cmd);
    }
}

fn split_whitespace(str: &str) -> (&str, &str) {
    let str = str.trim();
    str.find(char::is_whitespace)
        .map_or((str, ""), |n| (&str[..n], str[n + 1..].trim()))
}

fn do_help(_args: &str) {
    axlog::ax_println!("Available commands:");
    for (name, desc, _) in CMD_TABLE {
        axlog::ax_print!("  {}", name);
        axlog::ax_println!("  {}", desc);
    }
}

fn do_exit(_args: &str) {
    axlog::ax_println!("======== ArceBoot will exit and shut down ========");
    axhal::misc::terminate();
}

fn do_env(_args: &str) {
    axlog::ax_println!("======== ArceBoot env arguments ========");
    unsafe {
        let mut parser = crate::dtb::DtbParser::new(crate::dtb::GLOBAL_NOW_DTB_ADDRESS).unwrap();

        // Bootargs
        if !parser.read_property_value("/chosen", "bootargs") {
            error!("Read bootargs failed!");
        }
    }
    axlog::ax_println!("======== ArceBoot env arguments ========");
}

fn do_set_env(_args: &str) {
    unsafe {
        let mut parser = crate::dtb::DtbParser::new(crate::dtb::GLOBAL_NOW_DTB_ADDRESS).unwrap();

        // Bootargs
        if !parser.modify_property("/chosen", "bootargs", "console=ttyS0,115200") {
            error!("Change bootargs failed!");
        }

        // Save new dtb
        crate::dtb::GLOBAL_NOW_DTB_ADDRESS = parser.save_to_mem();
    }
}
