fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();

    if args.len() < 2 {
        return Err(format!("Usage: {} <enrypted_file>", args[0]).into())
    }

    let path = &args[1];
    let file = std::fs::read(path)?;

    eprint!("Type encryption key followed by enter: ");

    let mut buffer = String::new();
    std::io::stdin().read_line(&mut buffer)?;
    let buffer = buffer.trim();

    let data = model::EncryptedBackupData::decrypt(&file, &buffer).map_err(|_| "Invalid key")?;
    eprintln!("{:#?}", data);

    Ok(())
}