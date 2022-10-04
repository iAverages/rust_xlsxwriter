// Test case that compares a file generated by rust_xlsxwriter with a file
// created by Excel.
//
// SPDX-License-Identifier: MIT OR Apache-2.0 Copyright 2022, John McNamara,
// jmcnamara@cpan.org

use rust_xlsxwriter::{Workbook, XlsxError};

mod common;

// Test case to test TODO.
//
fn create_new_xlsx_file(filename: &str) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new(filename);

    let worksheet = workbook.add_worksheet();

    worksheet.write_formula_only(0, 0, "=1+_xlfn.XOR(1)")?;
    worksheet.set_formula_result(0, 0, "2");

    workbook.close()?;

    Ok(())
}

#[test]
fn test_dynamic_array03() {
    let testcase = "dynamic_array03";

    let (excel_file, xlsxwriter_file) = common::get_xlsx_filenames(testcase);
    _ = create_new_xlsx_file(&xlsxwriter_file);
    common::assert_eq(&excel_file, &xlsxwriter_file);
    common::remove_test_xlsx_file(&xlsxwriter_file);
}