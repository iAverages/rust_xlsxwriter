// shared_strings - A module for creating the Excel sharedStrings.xml file.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2022, John McNamara, jmcnamara@cpan.org

use crate::shared_strings_table::SharedStringsTable;
use crate::xmlwriter::XMLWriter;
use itertools::Itertools;

pub struct SharedStrings<'a> {
    pub writer: &'a mut XMLWriter<'a>,
}

impl<'a> SharedStrings<'a> {
    // Create a new struct to to track Excel shared strings between worksheets.
    pub fn new(writer: &'a mut XMLWriter<'a>) -> SharedStrings<'a> {
        SharedStrings { writer }
    }

    //  Assemble and write the XML file.
    pub fn assemble_xml_file(&mut self, string_table: &SharedStringsTable) {
        self.writer.xml_declaration();

        // Write the sst element.
        self.write_sst(string_table);

        // Write the sst strings.
        self.write_sst_strings(string_table);

        // Close the sst tag.
        self.writer.xml_end_tag("sst");
    }

    // Write the <sst> element.
    fn write_sst(&mut self, string_table: &SharedStringsTable) {
        let xmls = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
        let count = string_table.count.to_string();
        let unique = string_table.unique_count.to_string();
        let attributes = vec![("xmlns", xmls), ("count", &count), ("uniqueCount", &unique)];

        self.writer.xml_start_tag_attr("sst", &attributes);
    }

    // Write the sst string elements.
    fn write_sst_strings(&mut self, string_table: &SharedStringsTable) {
        for (string, _) in string_table.strings.iter().sorted_by_key(|x| x.1) {
            let preserve_whitespace =
                string.starts_with(char::is_whitespace) || string.ends_with(char::is_whitespace);

            self.writer.xml_si_element(string, preserve_whitespace);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::SharedStrings;
    use super::SharedStringsTable;
    use super::XMLWriter;

    use pretty_assertions::assert_eq;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use tempfile::tempfile;

    // Convert XML string/doc into a vector for comparison testing.
    pub fn xml_to_vec(xml_string: &str) -> Vec<String> {
        let mut xml_elements: Vec<String> = Vec::new();
        let re = regex::Regex::new(r">\s*<").unwrap();
        let tokens: Vec<&str> = re.split(xml_string).collect();

        for token in &tokens {
            let mut element = token.trim().to_string();

            // Add back the removed brackets.
            if !element.starts_with('<') {
                element = format!("<{}", element);
            }
            if !element.ends_with('>') {
                element = format!("{}>", element);
            }

            xml_elements.push(element);
        }
        xml_elements
    }

    // Test helper to read xml data back from a filehandle.
    fn read_xmlfile_data(tempfile: &mut File) -> String {
        let mut got = String::new();
        tempfile.seek(SeekFrom::Start(0)).unwrap();
        tempfile.read_to_string(&mut got).unwrap();
        got
    }

    #[test]
    fn test_shared_string_table() {
        let mut tempfile = tempfile().unwrap();
        let mut writer = XMLWriter::new(&tempfile);
        let mut string_table = SharedStringsTable::new();

        let mut shared_strings = SharedStrings::new(&mut writer);

        string_table.get_shared_string_index("neptune");
        string_table.get_shared_string_index("neptune");
        string_table.get_shared_string_index("neptune");
        string_table.get_shared_string_index("neptune");
        string_table.get_shared_string_index("mars");
        string_table.get_shared_string_index("venus");
        string_table.get_shared_string_index("mars");

        shared_strings.assemble_xml_file(&string_table);

        let got = read_xmlfile_data(&mut tempfile);
        let got = xml_to_vec(&got);

        let expected = xml_to_vec(
            r#"
                <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
                <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="7" uniqueCount="3">
                  <si>
                    <t>neptune</t>
                  </si>
                  <si>
                    <t>mars</t>
                  </si>
                  <si>
                    <t>venus</t>
                  </si>
                </sst>
                "#,
        );

        assert_eq!(got, expected);
    }

    #[test]
    fn test_shared_string_table_with_preserve() {
        let mut tempfile = tempfile().unwrap();
        let mut writer = XMLWriter::new(&tempfile);
        let mut string_table = SharedStringsTable::new();

        let mut shared_strings = SharedStrings::new(&mut writer);

        string_table.get_shared_string_index("abcdefg");
        string_table.get_shared_string_index("   abcdefg");
        string_table.get_shared_string_index("abcdefg   ");

        shared_strings.assemble_xml_file(&string_table);

        let got = read_xmlfile_data(&mut tempfile);
        let got = xml_to_vec(&got);

        let expected = xml_to_vec(
            r#"
                <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
                <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="3">
                <si>
                    <t>abcdefg</t>
                </si>
                <si>
                    <t xml:space="preserve">   abcdefg</t>
                </si>
                <si>
                    <t xml:space="preserve">abcdefg   </t>
                </si>
            </sst>
                "#,
        );

        assert_eq!(got, expected);
    }
}