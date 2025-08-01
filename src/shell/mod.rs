use axio::{Read, Write};

mod cmd;
mod stdio;

const LF: u8 = b'\n';
const CR: u8 = b'\r';
const DL: u8 = b'\x7f';
const BS: u8 = b'\x08';
const SPACE: u8 = b' ';

const MAX_CMD_LEN: usize = 256;

fn print_prompt() {
    axlog::ax_print!("[Arceboot]: {}$ ", &crate::medium::virtio_disk::current_dir().unwrap());
}

pub fn shell_main() {
    let mut stdin = self::stdio::stdin();
    let mut stdout = self::stdio::stdout();

    let mut buf = [0; MAX_CMD_LEN];
    let mut cursor = 0;

    // Attempt to autoboot
    const COUNTDOWN_MSG: &[u8] =
        b"[Arceboot] Attempt to autoboot, print any key to stop it, will start autoboot in: ";
    for i in (0..=5).rev() {
        if stdin.read_nb(&mut buf[cursor..cursor + 1]).ok() == Some(1) {
            break;
        }
        stdout.write_all(COUNTDOWN_MSG).unwrap();

        let num_char = b'0' + i;
        stdout.write_all(&[num_char, b' ']).unwrap();

        axhal::time::busy_wait(core::time::Duration::new(1, 0));

        if i > 0 {
            stdout.write_all(&[CR]).unwrap();
        }

        if i == 0 {
            autoboot();
            // boot 完成后理论上不应该回到程序
            return;
        }
    }
    axlog::ax_println!();

    // Cannot autoboot or cancel autoboot
    self::cmd::run_cmd("help".as_bytes());
    print_prompt();

    loop {
        if stdin.read(&mut buf[cursor..cursor + 1]).ok() != Some(1) {
            continue;
        }
        if buf[cursor] == b'\x1b' {
            buf[cursor] = b'^';
        }
        match buf[cursor] {
            CR | LF => {
                axlog::ax_println!();
                if cursor > 0 {
                    cmd::run_cmd(&buf[..cursor]);
                    cursor = 0;
                }
                print_prompt();
            }
            BS | DL => {
                if cursor > 0 {
                    stdout.write_all(&[BS, SPACE, BS]).unwrap();
                    cursor -= 1;
                }
            }
            0..=31 => {}
            c => {
                if cursor < MAX_CMD_LEN - 1 {
                    stdout.write_all(&[c]).unwrap();
                    cursor += 1;
                }
            }
        }
    }
}

fn autoboot() {
    crate::runtime::efi_runtime_init();
}
