// Test case that compares a file generated by rust_xlsxwriter with a file
// created by Excel.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright 2022-2023, John McNamara, jmcnamara@cpan.org

use rust_xlsxwriter::{Chart, ChartFormat, ChartLine, ChartType, Color, Workbook, XlsxError};

#[macro_use]
extern crate lazy_static;

mod common;

// Create rust_xlsxwriter file to compare against Excel file.
fn create_new_xlsx_file(filename: &str) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();

    let worksheet = workbook.add_worksheet();

    // Add some test data for the chart(s).
    for row_num in 0..8 {
        for col_num in 0..6 {
            worksheet.write_number(row_num as u32, col_num as u16, 1)?;
        }
    }

    let mut chart = Chart::new(ChartType::LineStacked);
    chart.set_axis_ids(68411392, 68414848);

    chart
        .add_series()
        .set_values(("Sheet1", 0, 0, 7, 0))
        .set_format(ChartFormat::new().set_line(ChartLine::new().set_color(Color::Theme(9, 0))));
    chart
        .add_series()
        .set_values(("Sheet1", 0, 1, 7, 1))
        .set_format(ChartFormat::new().set_line(ChartLine::new().set_color(Color::Theme(9, 1))));
    chart
        .add_series()
        .set_values(("Sheet1", 0, 2, 7, 2))
        .set_format(ChartFormat::new().set_line(ChartLine::new().set_color(Color::Theme(9, 2))));
    chart
        .add_series()
        .set_values(("Sheet1", 0, 3, 7, 3))
        .set_format(ChartFormat::new().set_line(ChartLine::new().set_color(Color::Theme(9, 3))));
    chart
        .add_series()
        .set_values(("Sheet1", 0, 4, 7, 4))
        .set_format(ChartFormat::new().set_line(ChartLine::new().set_color(Color::Theme(9, 4))));
    chart
        .add_series()
        .set_values(("Sheet1", 0, 5, 7, 5))
        .set_format(ChartFormat::new().set_line(ChartLine::new().set_color(Color::Theme(9, 5))));

    worksheet.insert_chart(8, 7, &chart)?;

    workbook.save(filename)?;

    Ok(())
}

#[test]
fn test_chart_theme11() {
    let test_runner = common::TestRunner::new()
        .set_name("chart_theme11")
        .set_function(create_new_xlsx_file)
        .initialize();

    test_runner.assert_eq();
    test_runner.cleanup();
}
