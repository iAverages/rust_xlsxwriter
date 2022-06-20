// relationship - A module for creating the Excel .rel relationship file.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2022, John McNamara, jmcnamara@cpan.org

use crate::xmlwriter::XMLWriter;

pub struct Relationship<'a> {
    pub writer: &'a mut XMLWriter<'a>,
    relationships: Vec<(String, String, String)>,
    id_num: u16,
}

impl<'a> Relationship<'a> {
    // Create a new struct to to track Excel shared strings between worksheets.
    pub fn new(writer: &'a mut XMLWriter<'a>) -> Relationship<'a> {
        Relationship {
            writer,
            relationships: vec![],
            id_num: 1,
        }
    }

    // Add container relationship to XLSX .rels xml files.
    pub fn add_document_relationship(&mut self, rel_type: &str, target: &str) {
        let document_schema = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

        self.relationships.push((
            format!("{}{}", document_schema, rel_type),
            target.to_string(),
            String::from(""),
        ));
    }

    //  Assemble and write the XML file.
    pub fn assemble_xml_file(&mut self) {
        self.writer.xml_declaration();

        // Write the Relationships element.
        self.write_relationships();

        // Close the Relationships tag.
        self.writer.xml_end_tag("Relationships");
    }

    // Write the <Relationships> element.
    fn write_relationships(&mut self) {
        let xmlns = "http://schemas.openxmlformats.org/package/2006/relationships";
        let attributes = vec![("xmlns", xmlns)];

        self.writer.xml_start_tag_attr("Relationships", &attributes);

        for relationship in self.relationships.clone() {
            // Write the Relationship element.
            self.write_relationship(relationship);
        }
    }

    // Write the <Relationship> element.
    fn write_relationship(&mut self, relationship: (String, String, String)) {
        let r_id = format!("rId{}", self.id_num);
        let (rel_type, target, target_mode) = relationship;

        self.id_num += 1;

        let mut attributes = vec![
            ("Id", r_id.as_str()),
            ("Type", rel_type.as_str()),
            ("Target", target.as_str()),
        ];

        if !target_mode.is_empty() {
            attributes.push(("TargetMode", target_mode.as_str()));
        }

        self.writer.xml_empty_tag_attr("Relationship", &attributes);
    }
}

#[cfg(test)]
mod tests {

    use super::Relationship;
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
    fn test_assemble() {
        let mut tempfile = tempfile().unwrap();
        let mut writer = XMLWriter::new(&tempfile);

        let mut rels = Relationship::new(&mut writer);

        rels.add_document_relationship("/worksheet", "worksheets/sheet1.xml");
        rels.add_document_relationship("/theme", "theme/theme1.xml");
        rels.add_document_relationship("/styles", "styles.xml");
        rels.add_document_relationship("/sharedStrings", "sharedStrings.xml");
        rels.add_document_relationship("/calcChain", "calcChain.xml");

        rels.assemble_xml_file();

        let got = read_xmlfile_data(&mut tempfile);
        let got = xml_to_vec(&got);

        let expected = xml_to_vec(
            r#"
            <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
              <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
              <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
              <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
              <Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
              <Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain" Target="calcChain.xml"/>
            </Relationships>
            "#,
        );

        assert_eq!(got, expected);
    }
}