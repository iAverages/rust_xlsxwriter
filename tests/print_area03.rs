// Test case that compares a file generated by rust_xlsxwriter with a file
// created by Excel.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2022, John McNamara, jmcnamara@cpan.org

use rust_xlsxwriter::{Workbook, XlsxError};

mod common;

// Test the creation of a simple rust_xlsxwriter file with a print area.
fn create_new_xlsx_file(filename: &str) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new(filename);

    let worksheet = workbook.add_worksheet();
    worksheet.write_string_only(0, 0, "Foo")?;

    worksheet.set_print_area(0, 0, 0, 16_383)?;

    workbook.close()?;

    Ok(())
}

#[test]
fn test_print_area03() {
    let test_runner = common::TestRunner::new("print_area03").initialize();

    _ = create_new_xlsx_file(test_runner.output_file());

    test_runner.assert_eq();
    test_runner.cleanup();
}
