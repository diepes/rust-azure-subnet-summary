// cargo watch -x 'fmt' -x 'run'  // 'run -- --some-arg'

mod cmd;
mod graph_read_subnet_data;
pub mod ipv4;
//mod read_csv;
mod write_banner;

pub fn get_subnet_fill_gaps() -> Result<graph_read_subnet_data::Data, Box<dyn std::error::Error>> {
    let mut data = graph_read_subnet_data::read_subnet_cache().expect("Error running az cli graph");
    // Sort by subnet_cidr
    data.data.sort_by_key(|s| s.subnet_cidr);
    Ok(data)
}

fn _escape_csv_field(input: &str) -> String {
    if input.contains(',') || input.contains('"') {
        // If the string contains a comma or double quote, enclose it in double quotes
        // and escape any double quotes within the field.
        // also excel does not like spaces after comma between fields
        let escaped = input.replace("\"", "\"\"");
        format!("\"{}\"", escaped)
    } else {
        // If the string doesn't contain a comma or double quote, no need to enclose it.
        input.to_string()
    }
}
