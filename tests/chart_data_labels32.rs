// Test case that compares a file generated by rust_xlsxwriter with a file
// created by Excel.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright 2022-2023, John McNamara, jmcnamara@cpan.org

use rust_xlsxwriter::{Chart, ChartDataLabel, ChartFont, ChartType, Workbook, XlsxError};

#[macro_use]
extern crate lazy_static;

mod common;

// Create rust_xlsxwriter file to compare against Excel file.
fn create_new_xlsx_file(filename: &str) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();

    let worksheet = workbook.add_worksheet();

    // Add some test data for the chart(s).
    let data = [
        [1, 2, 3, 10],
        [2, 4, 6, 20],
        [3, 6, 9, 30],
        [4, 8, 12, 40],
        [5, 10, 15, 50],
    ];
    for (row_num, row_data) in data.iter().enumerate() {
        for (col_num, col_data) in row_data.iter().enumerate() {
            worksheet.write_number(row_num as u32, col_num as u16, *col_data)?;
        }
    }

    let data_labels = vec![ChartDataLabel::new()
        .set_value("33")
        .set_font(
            ChartFont::new()
                .set_color("#FF0000")
                .set_bold()
                .set_italic(),
        )
        .to_custom()];

    let mut chart = Chart::new(ChartType::Column);
    chart.set_axis_ids(71374336, 71414144);
    chart
        .add_series()
        .set_values(("Sheet1", 0, 0, 4, 0))
        .set_data_label(ChartDataLabel::new().show_value())
        .set_custom_data_labels(&data_labels);

    chart.add_series().set_values(("Sheet1", 0, 1, 4, 1));
    chart.add_series().set_values(("Sheet1", 0, 2, 4, 2));

    worksheet.insert_chart(8, 4, &chart)?;

    workbook.save(filename)?;

    Ok(())
}

#[test]
fn test_chart_data_labels32() {
    let test_runner = common::TestRunner::new()
        .set_name("chart_data_labels32")
        .set_function(create_new_xlsx_file)
        .initialize();

    test_runner.assert_eq();
    test_runner.cleanup();
}
