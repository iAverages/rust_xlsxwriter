// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright 2022-2023, John McNamara, jmcnamara@cpan.org

//! TODO.

use rust_xlsxwriter::{Workbook, XlsxError};
use serde::Serialize;

fn main() -> Result<(), XlsxError> {
    // Create a new Excel file object.
    let mut workbook = Workbook::new();

    // Add a worksheet to the workbook.
    let worksheet = workbook.add_worksheet();

    #[derive(Serialize)]
    struct MyStruct1 {
        logical: bool,
        number: i8,
    }

    let struct1 = MyStruct1 {
        logical: true,
        number: 123,
    };

    worksheet.add_serialize_headers(1, 5, &["logical", "number"])?;

    worksheet.serialize(&struct1)?;
    worksheet.serialize(&struct1)?;

    // Create a serializable test struct.
    #[derive(Serialize)]
    struct MyStruct {
        col1: Vec<u16>,
        col2: Vec<bool>,
    }

    let data = MyStruct {
        col1: vec![123, 456, 789],
        col2: vec![true, false, true],
    };

    worksheet.add_serialize_headers(0, 0, &["col1", "col2"])?;
    worksheet.serialize(&data)?;

    // Save the file to disk.
    workbook.save("serialize.xlsx")?;

    Ok(())
}
