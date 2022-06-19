mod app;
mod shared_strings;
mod shared_strings_table;
mod xmlwriter;

use crate::app::App;
use crate::shared_strings::SharedStrings;
use crate::shared_strings_table::SharedStringsTable;
use crate::xmlwriter::XMLWriter;
use tempfile::tempfile;

// Test function to excercise sub-modules.
pub fn assemble_all() {
    let tempfile = tempfile().unwrap();

    let mut writer = XMLWriter::new(&tempfile);
    let mut string_table = SharedStringsTable::new();
    let mut shared_strings = SharedStrings::new(&mut writer);
    string_table.get_shared_string_index("hello");
    shared_strings.assemble_xml_file(&string_table);

    let mut writer = XMLWriter::new(&tempfile);
    let mut app = App::new(&mut writer);
    app.add_heading_pair("Worksheets", 1);
    app.add_part_name("Sheet1");
    app.assemble_xml_file();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_lib_compiles() {
        assert_eq!(true, true);
    }
}
