use std::{fs::{File, OpenOptions}, io::{Read, Write}};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::time::SystemTime;

fn seconds_in_days(days: u64) -> u64 {
    days * 24 * 60 * 60
}

fn format_rfc3339(mut time: u64) -> String {
    let days_in_months = vec![31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let four_years = seconds_in_days(365) * 3 + seconds_in_days(366);
    let mut leftover = time % four_years;
    let left_years = leftover / seconds_in_days(365);
    leftover = leftover % seconds_in_days(365);
    time /= four_years;
    let years = time;
    let mut left_days = leftover / seconds_in_days(1);
    let mut left_months = if days_in_months[0] <= left_days {
        left_days -= days_in_months[0];
        2
    }else{
        1
    };
    while days_in_months[left_months-1] <= left_days {
        left_days -= days_in_months[left_months-1];
        left_months += 1;
    }
    let mut left_seconds = leftover % seconds_in_days(1);
    let left_hours = left_seconds / (60 * 60);
    left_seconds = left_seconds % (60 * 60);
    let left_minutes = left_seconds / 60;
    left_seconds = left_seconds % 60;
    format!("{:02}/{:02}/{} {:02}:{:02}:{:02}", left_days, left_months, 1970 + years * 4 + left_years, left_hours, left_minutes, left_seconds)
}

fn log(message_receiver: Receiver<String>) {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open("C:\\Users\\lucas\\Projects\\dc-bridge\\log.txt").unwrap();
    while let Ok(message) = message_receiver.recv() {
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let message = format!("({}) | {}\n", format_rfc3339(now), message);
        file.write_all(message.as_bytes()).unwrap();
    }
}

enum FileOperation {
    Read,
    Write(Vec<u8>),
}

fn file_reader(
    mut file: File,
    sender: Sender<String>,
    op_receiver: Receiver<FileOperation>,
    data_sender: Sender<Vec<u8>>,
) {
    let mut handshake_sent = false;
    while let Ok(operation) = op_receiver.recv() {
        match operation {
            FileOperation::Read => {
                sender.send("FILEREADER: Reading from pipe...".into()).unwrap();
                let mut header = [0u8; 8];
                if file.read_exact(&mut header).is_err() {
                    sender.send("FILEREADER: Pipe closed while reading header.".into()).unwrap();
                    break;
                }

                let length_bytes: [u8; 4] = header[4..8].try_into().unwrap();
                let length = u32::from_le_bytes(length_bytes);

                let mut buffer = vec![0; length as usize];
                if file.read_exact(&mut buffer).is_err() {
                    sender.send("FILEREADER: Pipe closed while reading payload.".into()).unwrap();
                    break;
                }

                let mut full_response = header.to_vec();
                full_response.append(&mut buffer);
                data_sender.send(full_response).unwrap();
            }
            FileOperation::Write(data) => {
                sender.send("FILEREADER: Writing to pipe...".into()).unwrap();
                let opcode: u32 = if !handshake_sent { 0 } else { 1 };
                let length = data.len() as u32;

                let mut header = [0u8; 8];
                header[0..4].copy_from_slice(&opcode.to_le_bytes());
                header[4..8].copy_from_slice(&length.to_le_bytes());

                if file.write_all(&header).is_err() || file.write_all(&data).is_err() {
                    sender.send("FILEREADER: Failed to write to pipe.".into()).unwrap();
                    break;
                }
                if !handshake_sent {
                    handshake_sent = true;
                }
            }
        }
    }
}

fn stdin_and_response_handler(
    sender: Sender<String>,
    op_sender: Sender<FileOperation>,
    data_receiver: Receiver<Vec<u8>>,
) {
    sender.send("STDIN_HANDLER: Thread started.".into()).unwrap();
    let mut header = [0u8; 8];
    let mut stdout = std::io::stdout();

    while let Ok(_) = std::io::stdin().read_exact(&mut header) {
        let length_bytes: [u8; 4] = header[4..8].try_into().unwrap();
        let length = u32::from_le_bytes(length_bytes);
        let mut buffer = vec![0; length as usize];

        if std::io::stdin().read_exact(&mut buffer).is_err() {
            break;
        }

        sender.send(format!("STDIN_HANDLER: Sending Write command with {} bytes.", buffer.len())).unwrap();
        op_sender.send(FileOperation::Write(buffer)).unwrap();

        sender.send("STDIN_HANDLER: Sending Read command.".into()).unwrap();
        op_sender.send(FileOperation::Read).unwrap();

        if let Ok(response_data) = data_receiver.recv() {
            sender.send(format!("STDIN_HANDLER: Received {} byte response. Writing to stdout.", response_data.len())).unwrap();
            if stdout.write_all(&response_data).is_err() {
                sender.send("STDIN_HANDLER: Failed to write to stdout.".into()).unwrap();
                break;
            }
            stdout.flush().unwrap();
        } else {
            sender.send("STDIN_HANDLER: Worker channel closed. Exiting.".into()).unwrap();
            break;
        }
    }
    sender.send("STDIN_HANDLER: Stdin closed. Exiting...".into()).unwrap();
}

fn main() {
    let (log_sender, log_receiver) = channel();
    let (op_sender, op_receiver) = channel();
    let (data_sender, data_receiver) = channel();

    std::thread::spawn(move || log(log_receiver));

    let args : Vec<String> = std::env::args().collect();
    let Some(n) = args.get(1) else {
        log_sender.send("Not enough arguments".into()).unwrap();
        panic!("Not enough arguments");
    };

    let pipe_name = format!("discord-ipc-{n}");
    let pipe_path = format!("\\\\.\\pipe\\{pipe_name}");
    let mut fd = None;

    log_sender.send(format!("Attempting to connect to named pipe {pipe_name}")).unwrap();
    for _ in 0..10 {
        if let Ok(file) = OpenOptions::new().read(true).write(true).open(&pipe_path) {
            fd = Some(file);
            break; // Exit loop on success
        } else {
            log_sender.send("Failed to connect. Retrying...".into()).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    let Some(file) = fd else {
        log_sender.send("Failed to connect after multiple retries.".into()).unwrap();
        panic!("Could not connect to Discord pipe.");
    };
    log_sender.send("Connected!".into()).unwrap();

    let file_reader_log = log_sender.clone();
    let stdin_log = log_sender.clone();

    std::thread::spawn(move || file_reader(file, file_reader_log, op_receiver, data_sender));
    std::thread::spawn(move || stdin_and_response_handler(stdin_log, op_sender, data_receiver));

    std::thread::park();
}
