// Some utility functions for the rust_xlsxwriter module.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2022, John McNamara, jmcnamara@cpan.org

use crate::worksheet::ColNum;
use crate::worksheet::RowNum;

// Convert a zero indexed column cell reference to a string.
pub fn col_to_name(col_num: ColNum) -> String {
    let mut col_name = "".to_string();

    let mut col_num = col_num + 1;

    while col_num > 0 {
        // Set remainder from 1 .. 26
        let mut remainder = col_num % 26;

        if remainder == 0 {
            remainder = 26;
        }

        // Convert the remainder to a character.
        let col_letter = char::from_u32(64u32 + remainder as u32).unwrap();

        // Accumulate the column letters, right to left.
        col_name = format!("{}{}", col_letter, col_name);

        // Get the next order of magnitude.
        col_num = (col_num - 1) / 26;
    }

    col_name
}

// Convert a zero indexed row and column cell reference to a A1 style string.
pub fn rowcol_to_cell(row_num: RowNum, col_num: ColNum) -> String {
    format!("{}{}", col_to_name(col_num), row_num + 1)
}

// Convert a zero indexed row and column cell reference to an absolute $A$1
// style string.
pub fn rowcol_to_cell_abs(row_num: RowNum, col_num: ColNum) -> String {
    format!("${}${}", col_to_name(col_num), row_num + 1)
}

// Convert zero indexed row and col cell references to a A1:B1 style range string.
pub fn cell_range(
    first_row: RowNum,
    first_col: ColNum,
    last_row: RowNum,
    last_col: ColNum,
) -> String {
    let range1 = rowcol_to_cell(first_row, first_col);
    let range2 = rowcol_to_cell(last_row, last_col);

    if range1 == range2 {
        range1
    } else {
        format!("{}:{}", range1, range2)
    }
}

// Convert zero indexed row and col cell references to an absolute $A$1:$B$1
// style range string.
pub fn cell_range_abs(
    first_row: RowNum,
    first_col: ColNum,
    last_row: RowNum,
    last_col: ColNum,
) -> String {
    let range1 = rowcol_to_cell_abs(first_row, first_col);
    let range2 = rowcol_to_cell_abs(last_row, last_col);

    if range1 == range2 {
        range1
    } else {
        format!("{}:{}", range1, range2)
    }
}

// Create a quoted version of a worksheet name. Excel single quotes worksheet
// names that contain spaces and some other characters.
pub fn quote_sheetname(sheetname: &str) -> String {
    let mut sheetname = sheetname.to_string();

    // Ignore strings that are already quoted.
    if !sheetname.starts_with('\'') {
        // double quote and other single quotes.
        sheetname = sheetname.replace('\'', "''");

        // Single quote the worksheet name if it contains any of the characters
        // that Excel quotes when using the name in a formula.
        if sheetname.contains(' ') || sheetname.contains('!') || sheetname.contains('\'') {
            sheetname = format!("'{}'", sheetname);
        }
    }

    sheetname
}

//
// Tests.
//
#[cfg(test)]
mod tests {

    use crate::utility;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_col_to_name() {
        let tests = vec![
            (0, "A"),
            (1, "B"),
            (2, "C"),
            (9, "J"),
            (24, "Y"),
            (25, "Z"),
            (26, "AA"),
            (254, "IU"),
            (255, "IV"),
            (256, "IW"),
            (16383, "XFD"),
            (16384, "XFE"),
        ];

        for (col_num, col_string) in tests {
            assert_eq!(col_string, utility::col_to_name(col_num));
        }
    }

    #[test]
    fn test_row_col_to_cell() {
        let tests = vec![
            (0, 0, "A1"),
            (0, 1, "B1"),
            (0, 2, "C1"),
            (0, 9, "J1"),
            (1, 0, "A2"),
            (2, 0, "A3"),
            (9, 0, "A10"),
            (1, 24, "Y2"),
            (7, 25, "Z8"),
            (9, 26, "AA10"),
            (1, 254, "IU2"),
            (1, 255, "IV2"),
            (1, 256, "IW2"),
            (0, 16383, "XFD1"),
            (1048576, 16384, "XFE1048577"),
        ];

        for (row_num, col_num, cell_string) in tests {
            assert_eq!(cell_string, utility::rowcol_to_cell(row_num, col_num));
        }
    }

    #[test]
    fn test_cell_range() {
        let tests = vec![
            (0, 0, 9, 0, "A1:A10"),
            (1, 2, 8, 2, "C2:C9"),
            (0, 0, 3, 4, "A1:E4"),
            (0, 0, 0, 0, "A1"),
            (0, 0, 0, 1, "A1:B1"),
            (0, 2, 0, 9, "C1:J1"),
            (1, 0, 2, 0, "A2:A3"),
            (9, 0, 1, 24, "A10:Y2"),
            (7, 25, 9, 26, "Z8:AA10"),
            (1, 254, 1, 255, "IU2:IV2"),
            (1, 256, 0, 16383, "IW2:XFD1"),
            (0, 0, 1048576, 16384, "A1:XFE1048577"),
        ];

        for (start_row, start_col, end_row, end_col, cell_range) in tests {
            assert_eq!(
                cell_range,
                utility::cell_range(start_row, start_col, end_row, end_col)
            );
        }
    }

    #[test]
    fn test_quote_sheetname() {
        let tests = vec![
            ("Sheet1", "Sheet1"),
            ("Sheet.2", "Sheet.2"),
            ("Sheet_3", "Sheet_3"),
            ("'Sheet4'", "'Sheet4'"),
            ("'Sheet 5'", "Sheet 5"),
            ("'Sheet!6'", "Sheet!6"),
            ("'Sheet''7'", "Sheet'7"),
            (
                "'a''''''''''''''''''''''''''''''''''''''''''''''''''''''''''b'",
                "a'''''''''''''''''''''''''''''b",
            ),
        ];

        for (exp, sheetname) in tests {
            assert_eq!(exp, utility::quote_sheetname(sheetname));
        }
    }
}
