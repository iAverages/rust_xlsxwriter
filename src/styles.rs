// styles - A module for creating the Excel styles.xml file.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright 2022-2023, John McNamara, jmcnamara@cpan.org

use crate::format::Format;
use crate::xmlwriter::XMLWriter;
use crate::{
    Alignment, Border, Fill, Font, FormatAlign, FormatBorder, FormatDiagonalBorder, FormatPattern,
    FormatScript, FormatUnderline, XlsxColor,
};

pub struct Styles<'a> {
    pub(crate) writer: XMLWriter,
    xf_formats: &'a Vec<Format>,
    font_count: u16,
    fill_count: u16,
    border_count: u16,
    num_formats: Vec<String>,
    has_hyperlink_style: bool,
    is_rich_string_style: bool,
}

impl<'a> Styles<'a> {
    // -----------------------------------------------------------------------
    // Crate public methods.
    // -----------------------------------------------------------------------

    // Create a new Styles struct.
    pub(crate) fn new(
        xf_formats: &Vec<Format>,
        font_count: u16,
        fill_count: u16,
        border_count: u16,
        num_formats: Vec<String>,
        has_hyperlink_style: bool,
        is_rich_string_style: bool,
    ) -> Styles {
        let writer = XMLWriter::new();

        Styles {
            writer,
            xf_formats,
            font_count,
            fill_count,
            border_count,
            num_formats,
            has_hyperlink_style,
            is_rich_string_style,
        }
    }

    // -----------------------------------------------------------------------
    // XML assembly methods.
    // -----------------------------------------------------------------------

    //  Assemble and write the XML file.
    pub(crate) fn assemble_xml_file(&mut self) {
        self.writer.xml_declaration();

        // Write the styleSheet element.
        self.write_style_sheet();

        // Write the numFmts element.
        self.write_num_fmts();

        // Write the fonts element.
        self.write_fonts();

        // Write the fills element.
        self.write_fills();

        // Write the borders element.
        self.write_borders();

        // Write the cellStyleXfs element.
        self.write_cell_style_xfs();

        // Write the cellXfs element.
        self.write_cell_xfs();

        // Write the cellStyles element.
        self.write_cell_styles();

        // Write the dxfs element.
        self.write_dxfs();

        // Write the tableStyles element.
        self.write_table_styles();

        // Close the styleSheet tag.
        self.writer.xml_end_tag("styleSheet");
    }

    // Write the <styleSheet> element.
    fn write_style_sheet(&mut self) {
        let attributes = [(
            "xmlns",
            "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
        )];

        self.writer.xml_start_tag("styleSheet", &attributes);
    }

    // Write the <fonts> element.
    fn write_fonts(&mut self) {
        let attributes = [("count", self.font_count.to_string())];

        self.writer.xml_start_tag("fonts", &attributes);

        // Write the cell font elements.
        for xf_format in self.xf_formats {
            // Write the font element.
            if xf_format.has_font {
                self.write_font(&xf_format.font);
            }
        }

        self.writer.xml_end_tag("fonts");
    }

    // Write the <font> element.
    pub(crate) fn write_font(&mut self, font: &Font) {
        if self.is_rich_string_style {
            self.writer.xml_start_tag_only("rPr");
        } else {
            self.writer.xml_start_tag_only("font");
        }

        if font.bold {
            self.writer.xml_empty_tag_only("b");
        }

        if font.italic {
            self.writer.xml_empty_tag_only("i");
        }

        if font.strikethrough {
            self.writer.xml_empty_tag_only("strike");
        }

        if font.underline != FormatUnderline::None {
            self.write_font_underline(font);
        }

        if font.script != FormatScript::None {
            self.write_vert_align(font);
        }
        // Write the sz element.
        self.write_font_size(font);

        // Write the color element.
        self.write_font_color(font);

        // Write the name element.
        self.write_font_name(font);

        // Write the family element.
        if font.family > 0 {
            self.write_font_family(font);
        }

        // Write the charset element.
        if font.charset > 0 {
            self.write_font_charset(font);
        }

        // Write the scheme element.
        self.write_font_scheme(font);

        if self.is_rich_string_style {
            self.writer.xml_end_tag("rPr");
        } else {
            self.writer.xml_end_tag("font");
        }
    }

    // Write the <sz> element.
    fn write_font_size(&mut self, font: &Font) {
        let attributes = [("val", font.size.as_str())];

        self.writer.xml_empty_tag("sz", &attributes);
    }

    // Write the <color> element.
    fn write_font_color(&mut self, font: &Font) {
        let mut attributes = vec![];

        match font.color {
            XlsxColor::Automatic => {
                // The color element is omitted for an Automatic color.
            }
            XlsxColor::Default => {
                attributes.push(("theme", "1".to_string()));
                self.writer.xml_empty_tag("color", &attributes);
            }
            _ => {
                attributes.append(&mut font.color.attributes());
                self.writer.xml_empty_tag("color", &attributes);
            }
        }
    }

    // Write the <name> element.
    fn write_font_name(&mut self, font: &Font) {
        let attributes = [("val", font.name.clone())];

        if self.is_rich_string_style {
            self.writer.xml_empty_tag("rFont", &attributes);
        } else {
            self.writer.xml_empty_tag("name", &attributes);
        }
    }

    // Write the <family> element.
    fn write_font_family(&mut self, font: &Font) {
        let attributes = [("val", font.family.to_string())];

        self.writer.xml_empty_tag("family", &attributes);
    }

    // Write the <charset> element.
    fn write_font_charset(&mut self, font: &Font) {
        let attributes = [("val", font.charset.to_string())];

        self.writer.xml_empty_tag("charset", &attributes);
    }

    // Write the <scheme> element.
    fn write_font_scheme(&mut self, font: &Font) {
        let mut attributes = vec![];

        if !font.scheme.is_empty() {
            attributes.push(("val", font.scheme.to_string()));
        } else {
            return;
        }

        self.writer.xml_empty_tag("scheme", &attributes);
    }

    // Write the <u> underline element.
    fn write_font_underline(&mut self, font: &Font) {
        let mut attributes = vec![];

        match font.underline {
            FormatUnderline::Double => {
                attributes.push(("val", "double".to_string()));
            }
            FormatUnderline::SingleAccounting => {
                attributes.push(("val", "singleAccounting".to_string()));
            }
            FormatUnderline::DoubleAccounting => {
                attributes.push(("val", "doubleAccounting".to_string()));
            }
            _ => {}
        }

        self.writer.xml_empty_tag("u", &attributes);
    }

    // Write the <vertAlign> element.
    fn write_vert_align(&mut self, font: &Font) {
        let mut attributes = vec![];

        match font.script {
            FormatScript::Superscript => {
                attributes.push(("val", "superscript".to_string()));
            }
            FormatScript::Subscript => {
                attributes.push(("val", "subscript".to_string()));
            }
            FormatScript::None => {}
        }

        self.writer.xml_empty_tag("vertAlign", &attributes);
    }

    // Write the <fills> element.
    fn write_fills(&mut self) {
        let attributes = [("count", self.fill_count.to_string())];

        self.writer.xml_start_tag("fills", &attributes);

        // Write the default fill elements.
        self.write_default_fill("none".to_string());
        self.write_default_fill("gray125".to_string());

        // Write the cell fill elements.
        for xf_format in self.xf_formats {
            // Write the fill element.
            if xf_format.has_fill {
                self.write_fill(&xf_format.fill);
            }
        }

        self.writer.xml_end_tag("fills");
    }

    // Write the default <fill> element.
    fn write_default_fill(&mut self, pattern: String) {
        let attributes = [("patternType", pattern)];

        self.writer.xml_start_tag_only("fill");
        self.writer.xml_empty_tag("patternFill", &attributes);
        self.writer.xml_end_tag("fill");
    }

    // Write the user defined <fill> element.
    fn write_fill(&mut self, fill: &Fill) {
        // Special handling for pattern only case.
        if fill.pattern != FormatPattern::None
            && (fill.background_color == XlsxColor::Default
                || fill.background_color == XlsxColor::Automatic)
            && (fill.foreground_color == XlsxColor::Default
                || fill.foreground_color == XlsxColor::Automatic)
        {
            self.write_default_fill(fill.pattern.to_string());
            return;
        }

        // Start the "fill" element.
        self.writer.xml_start_tag_only("fill");

        // Write the fill pattern.
        let attributes = [("patternType", fill.pattern.to_string())];
        self.writer.xml_start_tag("patternFill", &attributes);

        // Write the foreground color.
        if fill.foreground_color != XlsxColor::Default
            && fill.foreground_color != XlsxColor::Automatic
        {
            let attributes = fill.foreground_color.attributes();
            self.writer.xml_empty_tag("fgColor", &attributes);
        }

        // Write the background color.
        if fill.background_color == XlsxColor::Default
            || fill.background_color == XlsxColor::Automatic
        {
            let attributes = [("indexed", "64")];
            self.writer.xml_empty_tag("bgColor", &attributes);
        } else {
            let attributes = fill.background_color.attributes();
            self.writer.xml_empty_tag("bgColor", &attributes);
        }

        self.writer.xml_end_tag("patternFill");
        self.writer.xml_end_tag("fill");
    }

    // Write the <borders> element.
    fn write_borders(&mut self) {
        let attributes = [("count", self.border_count.to_string())];

        self.writer.xml_start_tag("borders", &attributes);

        // Write the cell border elements.
        for xf_format in self.xf_formats {
            // Write the border element.
            if xf_format.has_border {
                self.write_border(&xf_format.borders);
            }
        }

        self.writer.xml_end_tag("borders");
    }

    // Write the <border> element.
    fn write_border(&mut self, borders: &Border) {
        match borders.diagonal_type {
            FormatDiagonalBorder::None => {
                self.writer.xml_start_tag_only("border");
            }
            FormatDiagonalBorder::BorderUp => {
                let attributes = [("diagonalUp", "1")];
                self.writer.xml_start_tag("border", &attributes);
            }
            FormatDiagonalBorder::BorderDown => {
                let attributes = [("diagonalDown", "1")];
                self.writer.xml_start_tag("border", &attributes);
            }
            FormatDiagonalBorder::BorderUpDown => {
                let attributes = [("diagonalUp", "1"), ("diagonalDown", "1")];
                self.writer.xml_start_tag("border", &attributes);
            }
        }

        // Write the four border elements.
        self.write_sub_border("left", borders.left_style, borders.left_color);
        self.write_sub_border("right", borders.right_style, borders.right_color);
        self.write_sub_border("top", borders.top_style, borders.top_color);
        self.write_sub_border("bottom", borders.bottom_style, borders.bottom_color);
        self.write_sub_border("diagonal", borders.diagonal_style, borders.diagonal_color);

        self.writer.xml_end_tag("border");
    }

    // Write the <border> sub elements such as <right>, <top>, etc.
    fn write_sub_border(
        &mut self,
        border_type: &str,
        border_style: FormatBorder,
        border_color: XlsxColor,
    ) {
        if border_style == FormatBorder::None {
            self.writer.xml_empty_tag_only(border_type);
            return;
        }

        let mut attributes = vec![("style", border_style.to_string())];
        self.writer.xml_start_tag(border_type, &attributes);

        if border_color != XlsxColor::Default && border_color != XlsxColor::Automatic {
            attributes = border_color.attributes();
        } else {
            attributes = vec![("auto", "1".to_string())];
        }

        self.writer.xml_empty_tag("color", &attributes);

        self.writer.xml_end_tag(border_type);
    }

    // Write the <cellStyleXfs> element.
    fn write_cell_style_xfs(&mut self) {
        let mut count = 1;
        if self.has_hyperlink_style {
            count = 2;
        }

        let attributes = [("count", count.to_string())];

        self.writer.xml_start_tag("cellStyleXfs", &attributes);

        // Write the style xf elements.
        self.write_normal_style_xf();

        if self.has_hyperlink_style {
            self.write_hyperlink_style_xf();
        }

        self.writer.xml_end_tag("cellStyleXfs");
    }

    // Write the style <xf> element for the "Normal" style.
    fn write_normal_style_xf(&mut self) {
        let attributes = [
            ("numFmtId", "0"),
            ("fontId", "0"),
            ("fillId", "0"),
            ("borderId", "0"),
        ];

        self.writer.xml_empty_tag("xf", &attributes);
    }

    // Write the style <xf> element for the "Hyperlink" style.
    fn write_hyperlink_style_xf(&mut self) {
        let attributes = [
            ("numFmtId", "0"),
            ("fontId", "1"),
            ("fillId", "0"),
            ("borderId", "0"),
            ("applyNumberFormat", "0"),
            ("applyFill", "0"),
            ("applyBorder", "0"),
            ("applyAlignment", "0"),
            ("applyProtection", "0"),
        ];

        self.writer.xml_start_tag("xf", &attributes);
        self.write_hyperlink_alignment();
        self.write_hyperlink_protection();
        self.writer.xml_end_tag("xf");
    }

    // Write the <alignment> element for hyperlinks.
    fn write_hyperlink_alignment(&mut self) {
        let attributes = [("vertical", "top")];

        self.writer.xml_empty_tag("alignment", &attributes);
    }

    // Write the <protection> element for hyperlinks.
    fn write_hyperlink_protection(&mut self) {
        let attributes = [("locked", "0")];

        self.writer.xml_empty_tag("protection", &attributes);
    }

    // Write the <cellXfs> element.
    fn write_cell_xfs(&mut self) {
        let xf_count = format!("{}", self.xf_formats.len());
        let attributes = [("count", xf_count)];

        self.writer.xml_start_tag("cellXfs", &attributes);

        // Write the cell xf element.
        for xf_format in self.xf_formats {
            self.write_cell_xf(xf_format);
        }

        self.writer.xml_end_tag("cellXfs");
    }

    // Write the cell <xf> element.
    fn write_cell_xf(&mut self, xf_format: &Format) {
        let has_protection = xf_format.has_protection();
        let has_alignment = xf_format.has_alignment();
        let apply_alignment = xf_format.apply_alignment();
        let is_hyperlink = xf_format.font.is_hyperlink;
        let xf_id = i32::from(is_hyperlink);

        let mut attributes = vec![
            ("numFmtId", xf_format.num_format_index.to_string()),
            ("fontId", xf_format.font_index.to_string()),
            ("fillId", xf_format.fill_index.to_string()),
            ("borderId", xf_format.border_index.to_string()),
            ("xfId", xf_id.to_string()),
        ];

        if xf_format.quote_prefix {
            attributes.push(("quotePrefix", "1".to_string()));
        }

        if xf_format.num_format_index > 0 {
            attributes.push(("applyNumberFormat", "1".to_string()));
        }

        if xf_format.font_index > 0 && !is_hyperlink {
            attributes.push(("applyFont", "1".to_string()));
        }

        if xf_format.fill_index > 0 {
            attributes.push(("applyFill", "1".to_string()));
        }

        if xf_format.border_index > 0 {
            attributes.push(("applyBorder", "1".to_string()));
        }

        if apply_alignment || is_hyperlink {
            attributes.push(("applyAlignment", "1".to_string()));
        }

        if has_protection || is_hyperlink {
            attributes.push(("applyProtection", "1".to_string()));
        }

        if has_alignment || has_protection {
            self.writer.xml_start_tag("xf", &attributes);

            if has_alignment {
                // Write the alignment element.
                self.write_alignment(xf_format.alignment);
            }

            if has_protection {
                // Write the protection element.
                self.write_protection(xf_format);
            }

            self.writer.xml_end_tag("xf");
        } else {
            self.writer.xml_empty_tag("xf", &attributes);
        }
    }

    // Write the <protection> element.
    fn write_protection(&mut self, xf_format: &Format) {
        let mut attributes = vec![];

        if !xf_format.locked {
            attributes.push(("locked", "0".to_string()));
        }

        if xf_format.hidden {
            attributes.push(("hidden", "1".to_string()));
        }

        self.writer.xml_empty_tag("protection", &attributes);
    }

    // Write the <alignment> element.
    fn write_alignment(&mut self, alignment: Alignment) {
        let mut attributes = vec![];
        let mut horizontal_align = alignment.horizontal;
        let mut shrink = alignment.shrink;

        // Indent is only allowed for horizontal "left", "right" and
        // "distributed". If it is defined for any other alignment or no
        // alignment has been set then default to left alignment.
        if alignment.indent > 0
            && horizontal_align != FormatAlign::Left
            && horizontal_align != FormatAlign::Right
            && horizontal_align != FormatAlign::Distributed
        {
            horizontal_align = FormatAlign::Left;
        }

        // Check for properties that are mutually exclusive with "shrink".
        if alignment.text_wrap
            || horizontal_align == FormatAlign::Fill
            || horizontal_align == FormatAlign::Justify
            || horizontal_align == FormatAlign::Distributed
        {
            shrink = false;
        }

        // Set the various attributes for horizontal alignment.
        match horizontal_align {
            FormatAlign::Center => {
                attributes.push(("horizontal", "center".to_string()));
            }
            FormatAlign::CenterAcross => {
                attributes.push(("horizontal", "centerContinuous".to_string()));
            }
            FormatAlign::Distributed => {
                attributes.push(("horizontal", "distributed".to_string()));
            }
            FormatAlign::Fill => {
                attributes.push(("horizontal", "fill".to_string()));
            }
            FormatAlign::Justify => {
                attributes.push(("horizontal", "justify".to_string()));
            }
            FormatAlign::Left => {
                attributes.push(("horizontal", "left".to_string()));
            }
            FormatAlign::Right => {
                attributes.push(("horizontal", "right".to_string()));
            }
            _ => {}
        }

        // Set the various attributes for vertical alignment.
        match alignment.vertical {
            FormatAlign::VerticalCenter => {
                attributes.push(("vertical", "center".to_string()));
            }
            FormatAlign::VerticalDistributed => {
                attributes.push(("vertical", "distributed".to_string()));
            }
            FormatAlign::VerticalJustify => {
                attributes.push(("vertical", "justify".to_string()));
            }
            FormatAlign::Top => {
                attributes.push(("vertical", "top".to_string()));
            }
            _ => {}
        }

        // Set other alignment properties.
        if alignment.indent != 0 {
            attributes.push(("indent", alignment.indent.to_string()));
        }

        if alignment.rotation != 0 {
            attributes.push(("textRotation", alignment.rotation.to_string()));
        }

        if alignment.text_wrap {
            attributes.push(("wrapText", "1".to_string()));
        }

        if shrink {
            attributes.push(("shrinkToFit", "1".to_string()));
        }

        if alignment.reading_direction > 0 && alignment.reading_direction <= 2 {
            attributes.push(("readingOrder", alignment.reading_direction.to_string()));
        }

        self.writer.xml_empty_tag("alignment", &attributes);
    }

    // Write the <cellStyles> element.
    fn write_cell_styles(&mut self) {
        let mut count = 1;
        if self.has_hyperlink_style {
            count = 2;
        }

        let attributes = [("count", count.to_string())];

        self.writer.xml_start_tag("cellStyles", &attributes);

        // Write the cellStyle elements.
        if self.has_hyperlink_style {
            self.write_hyperlink_cell_style();
        }
        self.write_normal_cell_style();

        self.writer.xml_end_tag("cellStyles");
    }

    // Write the <cellStyle> element for the "Normal" style.
    fn write_normal_cell_style(&mut self) {
        let attributes = [("name", "Normal"), ("xfId", "0"), ("builtinId", "0")];

        self.writer.xml_empty_tag("cellStyle", &attributes);
    }

    // Write the <cellStyle> element for the "Hyperlink" style.
    fn write_hyperlink_cell_style(&mut self) {
        let attributes = [("name", "Hyperlink"), ("xfId", "1"), ("builtinId", "8")];

        self.writer.xml_empty_tag("cellStyle", &attributes);
    }

    // Write the <dxfs> element.
    fn write_dxfs(&mut self) {
        let attributes = [("count", "0")];

        self.writer.xml_empty_tag("dxfs", &attributes);
    }

    // Write the <tableStyles> element.
    fn write_table_styles(&mut self) {
        let attributes = [
            ("count", "0"),
            ("defaultTableStyle", "TableStyleMedium9"),
            ("defaultPivotStyle", "PivotStyleLight16"),
        ];

        self.writer.xml_empty_tag("tableStyles", &attributes);
    }

    // Write the <numFmts> element.
    fn write_num_fmts(&mut self) {
        if self.num_formats.is_empty() {
            return;
        }

        let attributes = [("count", self.num_formats.len().to_string())];
        self.writer.xml_start_tag("numFmts", &attributes);

        // Write the numFmt elements.
        for (index, num_format) in self.num_formats.clone().iter().enumerate() {
            self.write_num_fmt(index as u16 + 164, num_format);
        }

        self.writer.xml_end_tag("numFmts");
    }

    // Write the <numFmt> element.
    fn write_num_fmt(&mut self, num_format_index: u16, num_format: &str) {
        let attributes = [
            ("numFmtId", num_format_index.to_string()),
            ("formatCode", num_format.to_string()),
        ];

        self.writer.xml_empty_tag("numFmt", &attributes);
    }
}

// -----------------------------------------------------------------------
// Tests.
// -----------------------------------------------------------------------
#[cfg(test)]
mod tests {

    use crate::styles::Styles;
    use crate::test_functions::xml_to_vec;
    use crate::Format;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_assemble() {
        let mut xf_format = Format::new();
        xf_format.set_font_index(0, true);
        xf_format.set_border_index(0, true);

        let xf_formats = vec![xf_format];
        let mut styles = Styles::new(&xf_formats, 1, 2, 1, vec![], false, false);

        styles.assemble_xml_file();

        let got = styles.writer.read_to_str();
        let got = xml_to_vec(got);

        let expected = xml_to_vec(
            r#"
                <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
                <styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                <fonts count="1">
                    <font>
                    <sz val="11"/>
                    <color theme="1"/>
                    <name val="Calibri"/>
                    <family val="2"/>
                    <scheme val="minor"/>
                    </font>
                </fonts>
                <fills count="2">
                    <fill>
                    <patternFill patternType="none"/>
                    </fill>
                    <fill>
                    <patternFill patternType="gray125"/>
                    </fill>
                </fills>
                <borders count="1">
                    <border>
                    <left/>
                    <right/>
                    <top/>
                    <bottom/>
                    <diagonal/>
                    </border>
                </borders>
                <cellStyleXfs count="1">
                    <xf numFmtId="0" fontId="0" fillId="0" borderId="0"/>
                </cellStyleXfs>
                <cellXfs count="1">
                    <xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
                </cellXfs>
                <cellStyles count="1">
                    <cellStyle name="Normal" xfId="0" builtinId="0"/>
                </cellStyles>
                <dxfs count="0"/>
                <tableStyles count="0" defaultTableStyle="TableStyleMedium9" defaultPivotStyle="PivotStyleLight16"/>
                </styleSheet>
                "#,
        );

        assert_eq!(expected, got);
    }
}
