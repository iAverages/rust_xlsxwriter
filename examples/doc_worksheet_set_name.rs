// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2022, John McNamara, jmcnamara@cpan.org

//! The following example demonstrates setting user defined worksheet names
//! and the default values when a name isn't set.

use rust_xlsxwriter::{Workbook, XlsxError};

fn main() -> Result<(), XlsxError> {
    let mut workbook = Workbook::new("worksheets.xlsx");

    _ = workbook.add_worksheet(); // Sheet1
    _ = workbook.add_worksheet().set_name("Foglio2"); // Foglio2
    _ = workbook.add_worksheet().set_name("Data"); // Data
    _ = workbook.add_worksheet(); // Sheet4

    workbook.close()?;

    Ok(())
}