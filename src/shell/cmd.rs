type CmdHandler = fn(&str);

const CMD_TABLE: &[(&str, &str, CmdHandler)] = &[
    ("help", "-- List the help for ArceBoot", do_help),
    ("exit", "-- Exit ArceBoot", do_exit),
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
