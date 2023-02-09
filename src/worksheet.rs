// worksheet - A module for creating the Excel sheet.xml file.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright 2022-2023, John McNamara, jmcnamara@cpan.org

#![warn(missing_docs)]
use std::borrow::Cow;
use std::cmp;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::mem;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use itertools::Itertools;
use regex::Regex;

use crate::drawing::{Drawing, DrawingCoordinates, DrawingInfo, DrawingObject};
use crate::error::XlsxError;
use crate::format::Format;
use crate::shared_strings_table::SharedStringsTable;
use crate::styles::Styles;
use crate::vml::VmlInfo;
use crate::xmlwriter::XMLWriter;
use crate::{utility, HeaderImagePosition, Image, ObjectMovement, ProtectionOptions, XlsxColor};
use crate::{Chart, ChartSeriesCacheData};
use crate::{FilterCondition, FilterCriteria, FilterData, FilterDataType};

/// Integer type to represent a zero indexed row number. Excel's limit for rows
/// in a worksheet is 1,048,576.
pub type RowNum = u32;

/// Integer type to represent a zero indexed column number. Excel's limit for
/// columns in a worksheet is 16,384.
pub type ColNum = u16;

const COL_MAX: ColNum = 16_384;
const ROW_MAX: RowNum = 1_048_576;
const MAX_URL_LEN: usize = 2_080;
const MAX_STRING_LEN: usize = 32_767;
const MAX_PARAMETER_LEN: usize = 255;
const DEFAULT_COL_WIDTH: f64 = 8.43;
const DEFAULT_ROW_HEIGHT: f64 = 15.0;
pub(crate) const NUM_IMAGE_FORMATS: usize = 5;

/// The Worksheet struct represents an Excel worksheet. It handles operations
/// such as writing data to cells or formatting the worksheet layout.
///
/// <img src="https://rustxlsxwriter.github.io/images/demo.png">
///
/// # Examples
///
/// Sample code to generate the Excel file shown above.
///
/// ```rust
/// # // This code is available in examples/app_demo.rs
/// #
/// use chrono::NaiveDate;
/// use rust_xlsxwriter::{Format, Image, Workbook, FormatAlign, FormatBorder, XlsxError};
///
/// fn main() -> Result<(), XlsxError> {
///     // Create a new Excel file object.
///     let mut workbook = Workbook::new();
///
///     // Create some formats to use in the worksheet.
///     let bold_format = Format::new().set_bold();
///     let decimal_format = Format::new().set_num_format("0.000");
///     let date_format = Format::new().set_num_format("yyyy-mm-dd");
///     let merge_format = Format::new()
///         .set_border(FormatBorder::Thin)
///         .set_align(FormatAlign::Center);
///
///     // Add a worksheet to the workbook.
///     let worksheet = workbook.add_worksheet();
///
///     // Set the column width for clarity.
///     worksheet.set_column_width(0, 22)?;
///
///     // Write a string without formatting.
///     worksheet.write_string(0, 0, "Hello")?;
///
///     // Write a string with the bold format defined above.
///     worksheet.write_string_with_format(1, 0, "World", &bold_format)?;
///
///     // Write some numbers.
///     worksheet.write_number(2, 0, 1)?;
///     worksheet.write_number(3, 0, 2.34)?;
///
///     // Write a number with formatting.
///     worksheet.write_number_with_format(4, 0, 3.00, &decimal_format)?;
///
///     // Write a formula.
///     worksheet.write_formula(5, 0, "=SIN(PI()/4)")?;
///
///     // Write a date.
///     let date = NaiveDate::from_ymd_opt(2023, 1, 25).unwrap();
///     worksheet.write_date(6, 0, &date, &date_format)?;
///
///     // Write some links.
///     worksheet.write_url(7, 0, "https://www.rust-lang.org")?;
///     worksheet.write_url_with_text(8, 0, "https://www.rust-lang.org", "Learn Rust")?;
///
///     // Write some merged cells.
///     worksheet.merge_range(9, 0, 9, 1, "Merged cells", &merge_format)?;
///
///     // Insert an image.
///     let image = Image::new("examples/rust_logo.png")?;
///     worksheet.insert_image(1, 2, &image)?;
///
///     // Save the file to disk.
///     workbook.save("demo.xlsx")?;
///
///     Ok(())
/// }
/// ```
pub struct Worksheet {
    pub(crate) writer: XMLWriter,
    pub(crate) name: String,
    pub(crate) active: bool,
    pub(crate) selected: bool,
    pub(crate) hidden: bool,
    pub(crate) first_sheet: bool,
    pub(crate) uses_string_table: bool,
    pub(crate) has_dynamic_arrays: bool,
    pub(crate) print_area_defined_name: DefinedName,
    pub(crate) repeat_row_cols_defined_name: DefinedName,
    pub(crate) autofilter_defined_name: DefinedName,
    pub(crate) autofilter_area: String,
    pub(crate) xf_formats: Vec<Format>,
    pub(crate) has_hyperlink_style: bool,
    pub(crate) hyperlink_relationships: Vec<(String, String, String)>,
    pub(crate) drawing_object_relationships: Vec<(String, String, String)>,
    pub(crate) drawing_relationships: Vec<(String, String, String)>,
    pub(crate) vml_drawing_relationships: Vec<(String, String, String)>,
    pub(crate) images: BTreeMap<(RowNum, ColNum), Image>,
    pub(crate) header_footer_vml_info: Vec<VmlInfo>,
    pub(crate) drawing: Drawing,
    pub(crate) image_types: [bool; NUM_IMAGE_FORMATS],
    pub(crate) header_footer_images: [Option<Image>; 6],
    pub(crate) charts: BTreeMap<(RowNum, ColNum), Chart>,

    table: HashMap<RowNum, HashMap<ColNum, CellType>>,
    merged_ranges: Vec<CellRange>,
    merged_cells: HashMap<(RowNum, ColNum), usize>,
    col_names: HashMap<ColNum, String>,
    dimensions: CellRange,
    xf_indices: HashMap<String, u32>,
    global_xf_indices: Vec<u32>,
    changed_rows: HashMap<RowNum, RowOptions>,
    changed_cols: HashMap<ColNum, ColOptions>,
    page_setup_changed: bool,
    tab_color: XlsxColor,
    fit_to_page: bool,
    fit_width: u16,
    fit_height: u16,
    paper_size: u8,
    default_page_order: bool,
    right_to_left: bool,
    portrait: bool,
    page_view: PageView,
    zoom: u16,
    print_scale: u16,
    print_options_changed: bool,
    center_horizontally: bool,
    center_vertically: bool,
    print_gridlines: bool,
    print_black_and_white: bool,
    print_draft: bool,
    print_headings: bool,
    header: String,
    footer: String,
    head_footer_changed: bool,
    header_footer_scale_with_doc: bool,
    header_footer_align_with_page: bool,
    margin_left: f64,
    margin_right: f64,
    margin_top: f64,
    margin_bottom: f64,
    margin_header: f64,
    margin_footer: f64,
    first_page_number: u16,
    default_result: String,
    use_future_functions: bool,
    panes: Panes,
    hyperlinks: BTreeMap<(RowNum, ColNum), Hyperlink>,
    rel_count: u16,
    protection_on: bool,
    protection_hash: u16,
    protection_options: ProtectionOptions,
    unprotected_ranges: Vec<(String, String, u16)>,
    selected_range: (String, String),
    top_left_cell: String,
    horizontal_breaks: Vec<u32>,
    vertical_breaks: Vec<u32>,
    filter_conditions: HashMap<ColNum, FilterCondition>,
    filter_automatic_off: bool,
    has_drawing_object_linkage: bool,
}

impl Default for Worksheet {
    fn default() -> Self {
        Self::new()
    }
}

impl Worksheet {
    // -----------------------------------------------------------------------
    // Public (and crate public) methods.
    // -----------------------------------------------------------------------

    /// Create a new Worksheet object to represent an Excel worksheet.
    ///
    /// The `Worksheet::new()` constructor is used to create a new Excel
    /// worksheet object. This can be used to write data to a worksheet prior to
    /// adding it to a workbook.
    ///
    /// There are two way of creating a worksheet object with rust_xlsxwriter:
    /// via the [`workbook.add_worksheet()`](crate::Workbook::add_worksheet)
    /// method and via the [`Worksheet::new()`] constructor. The first method
    /// ties the worksheet to the workbook object that will write it
    /// automatically when the file is saved, whereas the second method creates
    /// a worksheet that is independent of a workbook. This has certain
    /// advantages in keeping the worksheet free of the workbook borrow checking
    /// until you wish to add it.
    ///
    /// When working with an independent worksheet object you will need to add
    /// it to a workbook using
    /// [`workbook.push_worksheet`](crate::Workbook::push_worksheet) in order
    /// for it to be written to a file.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Creating worksheets]
    /// and working with the borrow checker.
    ///
    /// [Creating worksheets]: https://rustxlsxwriter.github.io/worksheet/create.html
    ///
    /// # Examples
    ///
    /// The following example demonstrates creating new worksheet objects and
    /// then adding them to a workbook.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_new.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    ///     // Create a new workbook.
    ///     let mut workbook = Workbook::new();
    ///
    ///     // Create new worksheets.
    ///     let mut worksheet1 = Worksheet::new();
    ///     let mut worksheet2 = Worksheet::new();
    ///
    ///     // Use the first workbook.
    ///     worksheet1.write_string(0, 0, "Hello")?;
    ///     worksheet1.write_string(1, 0, "Sheet1")?;
    ///
    ///     // Use the second workbook.
    ///     worksheet2.write_string(0, 0, "Hello")?;
    ///     worksheet2.write_string(1, 0, "Sheet2")?;
    ///
    ///     // Add the worksheets to the workbook.
    ///     workbook.push_worksheet(worksheet1);
    ///     workbook.push_worksheet(worksheet2);
    ///
    ///     // Save the workbook.
    ///     workbook.save("worksheets.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_new.png">
    ///
    pub fn new() -> Worksheet {
        let writer = XMLWriter::new();

        let default_format = Format::default();
        let xf_indices = HashMap::from([(default_format.format_key(), 0)]);

        // Initialize the min and max dimensions with their opposite value.
        let dimensions = CellRange {
            first_row: ROW_MAX,
            first_col: COL_MAX,
            last_row: 0,
            last_col: 0,
        };

        let panes = Panes {
            freeze_cell: (0, 0),
            top_cell: (0, 0),
        };

        Worksheet {
            writer,
            name: "".to_string(),
            active: false,
            selected: false,
            hidden: false,
            first_sheet: false,
            uses_string_table: false,
            has_dynamic_arrays: false,
            print_area_defined_name: DefinedName::new(),
            repeat_row_cols_defined_name: DefinedName::new(),
            autofilter_defined_name: DefinedName::new(),
            autofilter_area: "".to_string(),
            table: HashMap::new(),
            col_names: HashMap::new(),
            dimensions,
            merged_ranges: vec![],
            merged_cells: HashMap::new(),
            xf_formats: vec![default_format],
            xf_indices,
            global_xf_indices: vec![],
            changed_rows: HashMap::new(),
            changed_cols: HashMap::new(),
            page_setup_changed: false,
            fit_to_page: false,
            tab_color: XlsxColor::Automatic,
            fit_width: 1,
            fit_height: 1,
            paper_size: 0,
            default_page_order: true,
            right_to_left: false,
            portrait: true,
            page_view: PageView::Normal,
            zoom: 100,
            print_scale: 100,
            print_options_changed: false,
            center_horizontally: false,
            center_vertically: false,
            print_gridlines: false,
            print_black_and_white: false,
            print_draft: false,
            print_headings: false,
            header: "".to_string(),
            footer: "".to_string(),
            head_footer_changed: false,
            header_footer_scale_with_doc: true,
            header_footer_align_with_page: true,
            margin_left: 0.7,
            margin_right: 0.7,
            margin_top: 0.75,
            margin_bottom: 0.75,
            margin_header: 0.3,
            margin_footer: 0.3,
            first_page_number: 0,
            default_result: "0".to_string(),
            use_future_functions: false,
            panes,
            has_hyperlink_style: false,
            hyperlinks: BTreeMap::new(),
            hyperlink_relationships: vec![],
            drawing_object_relationships: vec![],
            drawing_relationships: vec![],
            vml_drawing_relationships: vec![],
            images: BTreeMap::new(),
            drawing: Drawing::new(),
            image_types: [false; NUM_IMAGE_FORMATS],
            header_footer_images: [None, None, None, None, None, None],
            header_footer_vml_info: vec![],
            rel_count: 0,
            protection_on: false,
            protection_hash: 0,
            protection_options: ProtectionOptions::new(),
            unprotected_ranges: vec![],
            selected_range: ("".to_string(), "".to_string()),
            top_left_cell: "".to_string(),
            horizontal_breaks: vec![],
            vertical_breaks: vec![],
            filter_conditions: HashMap::new(),
            filter_automatic_off: false,
            charts: BTreeMap::new(),
            has_drawing_object_linkage: false,
        }
    }

    /// Set the worksheet name.
    ///
    /// Set the worksheet name. If no name is set the default Excel convention
    /// will be followed (Sheet1, Sheet2, etc.) in the order the worksheets are
    /// created.
    ///
    /// # Arguments
    ///
    /// * `name` - The worksheet name. It must follow the Excel rules, shown
    ///   below.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::SheetnameCannotBeBlank`] - Worksheet name cannot be
    ///   blank.
    /// * [`XlsxError::SheetnameLengthExceeded`] - Worksheet name exceeds
    ///   Excel's limit of 31 characters.
    /// * [`XlsxError::SheetnameContainsInvalidCharacter`] - Worksheet name
    ///   cannot contain invalid characters: `[ ] : * ? / \`
    /// * [`XlsxError::SheetnameStartsOrEndsWithApostrophe`] - Worksheet name
    ///   cannot start or end with an apostrophe.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting user defined worksheet names
    /// and the default values when a name isn't set.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_name.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let _worksheet1 = workbook.add_worksheet(); // Defaults to Sheet1
    ///     let _worksheet2 = workbook.add_worksheet().set_name("Foglio2");
    ///     let _worksheet3 = workbook.add_worksheet().set_name("Data");
    ///     let _worksheet4 = workbook.add_worksheet(); // Defaults to Sheet4
    ///
    /// #     workbook.save("worksheets.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_name.png">
    ///
    /// The worksheet name must be a valid Excel worksheet name, i.e:
    ///
    /// * The name is less than 32 characters.
    /// * The name isn't blank.
    /// * The name doesn't contain any of the characters: `[ ] : * ? / \`.
    /// * The name doesn't start or end with an apostrophe.
    /// * The name shouldn't be "History" (case-insensitive) since that is
    ///   reserved by Excel.
    /// * It must not be a duplicate of another worksheet name used in the
    ///   workbook.
    ///
    /// The rules for worksheet names in Excel are explained in the [Microsoft
    /// Office documentation].
    ///
    /// [Microsoft Office documentation]:
    ///     https://support.office.com/en-ie/article/rename-a-worksheet-3f1f7148-ee83-404d-8ef0-9ff99fbad1f9
    ///
    pub fn set_name(&mut self, name: &str) -> Result<&mut Worksheet, XlsxError> {
        // Check that the sheet name isn't blank.
        if name.is_empty() {
            return Err(XlsxError::SheetnameCannotBeBlank);
        }

        // Check that sheet sheetname is <= 31, an Excel limit.
        if name.chars().count() > 31 {
            return Err(XlsxError::SheetnameLengthExceeded(name.to_string()));
        }

        // Check that sheetname doesn't contain any invalid characters.
        let re = Regex::new(r"[\[\]:*?/\\]").unwrap();
        if re.is_match(name) {
            return Err(XlsxError::SheetnameContainsInvalidCharacter(
                name.to_string(),
            ));
        }

        // Check that sheetname doesn't start or end with an apostrophe.
        if name.starts_with('\'') || name.ends_with('\'') {
            return Err(XlsxError::SheetnameStartsOrEndsWithApostrophe(
                name.to_string(),
            ));
        }

        self.name = name.to_string();

        Ok(self)
    }

    /// Get the worksheet name.
    ///
    /// Get the worksheet name that was set automatically such as Sheet1,
    /// Sheet2, etc., or that was set by the user using
    /// [`set_name()`](Worksheet::set_name).
    ///
    /// The worksheet name can be used to get a reference to a worksheet object
    /// using the
    /// [`workbook.worksheet_from_name()`](crate::Workbook::worksheet_from_name)
    /// method.
    ///
    /// # Examples
    ///
    /// The following example demonstrates getting a worksheet name.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_name.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     // Try name() using a default sheet name.
    ///     let worksheet = workbook.add_worksheet();
    ///     assert_eq!("Sheet1", worksheet.name());
    ///
    ///     // Try name() using a user defined sheet name.
    ///     let worksheet = workbook.add_worksheet().set_name("Data")?;
    ///     assert_eq!("Data", worksheet.name());
    ///
    /// #    workbook.save("workbook.xlsx")?;
    /// #
    /// #    Ok(())
    /// # }
    /// ```
    ///
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Write an unformatted number to a cell.
    ///
    /// Write an unformatted number to a worksheet cell. To write a formatted
    /// number see the
    /// [`write_number_with_format()`](Worksheet::write_number_with_format())
    /// method below.
    ///
    /// All numerical values in Excel are stored as [IEEE 754] Doubles which are
    /// the equivalent of rust's [`f64`] type. This method will accept any rust
    /// type that will convert [`Into`] a f64. These include i8, u8, i16, u16,
    /// i32, u32 and f32 but not i64 or u64. IEEE 754 Doubles and f64 have
    /// around 15 digits of precision. Anything beyond that cannot be stored by
    /// Excel as a number without loss of precision and may need to be stored as
    /// a string instead.
    ///
    /// [IEEE 754]: https://en.wikipedia.org/wiki/IEEE_754
    ///
    ///  Excel doesn't have handling for NaN or INF floating point numbers.
    ///  These will be stored as the strings "Nan", "INF", and "-INF" strings
    ///  instead.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `number` - The number to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing unformatted numbers to an
    /// Excel worksheet. Any numeric type that will convert [`Into`] f64 can be
    /// transferred to Excel.
    ///
    /// ```
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    ///     let mut workbook = Workbook::new();
    ///
    ///     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Write some different rust number types to a worksheet.
    ///     // Note, u64 isn't supported by Excel.
    ///     worksheet.write_number(0, 0, 1_u8)?;
    ///     worksheet.write_number(1, 0, 2_i16)?;
    ///     worksheet.write_number(2, 0, 3_u32)?;
    ///     worksheet.write_number(3, 0, 4_f32)?;
    ///     worksheet.write_number(4, 0, 5_f64)?;
    ///
    ///     // Write some numbers with implicit types.
    ///     worksheet.write_number(5, 0, 1234)?;
    ///     worksheet.write_number(6, 0, 1234.5)?;
    ///
    ///     // Note Excel normally ignores trailing decimal zeros
    ///     // when the number is unformatted.
    ///     worksheet.write_number(7, 0, 1234.50000)?;
    ///
    /// #     workbook.save("numbers.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_number.png">
    ///
    pub fn write_number<T>(
        &mut self,
        row: RowNum,
        col: ColNum,
        number: T,
    ) -> Result<&mut Worksheet, XlsxError>
    where
        T: Into<f64>,
    {
        // Store the cell data.
        self.store_number(row, col, number.into(), None)
    }

    /// Write a formatted number to a worksheet cell.
    ///
    /// Write a number with formatting to a worksheet cell. The format is set
    /// via a [`Format`] struct which can control the numerical formatting of
    /// the number, for example as a currency or a percentage value, or the
    /// visual format, such as bold and italic text.
    ///
    /// All numerical values in Excel are stored as [IEEE 754] Doubles which are
    /// the equivalent of rust's [`f64`] type. This method will accept any
    /// rust type that will convert [`Into`] a f64. These include i8, u8, i16,
    /// u16, i32, u32 and f32 but not i64 or u64. IEEE 754 Doubles and f64 have
    /// around 15 digits of precision. Anything beyond that cannot be stored by
    /// Excel as a number without loss of precision and may need to be stored as
    /// a string instead.
    ///
    /// [IEEE 754]: https://en.wikipedia.org/wiki/IEEE_754
    ///
    ///  Excel doesn't have handling for NaN or INF floating point numbers.
    ///  These will be stored as the strings "Nan", "INF", and "-INF" strings
    ///  instead.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `number` - The number to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting different formatting for
    /// numbers in an Excel worksheet.
    ///
    /// ```
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    ///     let mut workbook = Workbook::new();
    ///
    ///     // Create some formats to use with the numbers below.
    ///     let number_format = Format::new().set_num_format("#,##0.00");
    ///     let currency_format = Format::new().set_num_format("€#,##0.00");
    ///     let percentage_format = Format::new().set_num_format("0.0%");
    ///     let bold_italic_format = Format::new().set_bold().set_italic();
    ///
    ///     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     worksheet.write_number_with_format(0, 0, 1234.5, &number_format)?;
    ///     worksheet.write_number_with_format(1, 0, 1234.5, &currency_format)?;
    ///     worksheet.write_number_with_format(2, 0, 0.3300, &percentage_format)?;
    ///     worksheet.write_number_with_format(3, 0, 1234.5, &bold_italic_format)?;
    ///
    /// #     workbook.save("numbers.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_number_with_format.png">
    ///
    ///
    pub fn write_number_with_format<T>(
        &mut self,
        row: RowNum,
        col: ColNum,
        number: T,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError>
    where
        T: Into<f64>,
    {
        // Store the cell data.
        self.store_number(row, col, number.into(), Some(format))
    }

    /// Write an unformatted string to a worksheet cell.
    ///
    /// Write an unformatted string to a worksheet cell. To write a formatted
    /// string see the
    /// [`write_string_with_format()`](Worksheet::write_string_with_format())
    /// method below.
    ///
    /// Excel only supports UTF-8 text in the xlsx file format. Any rust UTF-8
    /// encoded string can be written with this method. The maximum string size
    /// supported by Excel is 32,767 characters.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `string` - The string to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxStringLengthExceeded`] - String exceeds Excel's limit
    ///   of 32,767 characters.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing some strings to a worksheet.
    /// The UTF-8 strings are taken from the UTF-8 example in the [Rust
    /// Programming Language] book.
    ///
    /// [Rust Programming Language]:
    ///     https://doc.rust-lang.org/book/ch08-02-strings.html#creating-a-new-string
    ///
    /// ```
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #   // Create a new Excel file object.
    /// #   let mut workbook = Workbook::new();
    /// #
    /// #   // Add a worksheet to the workbook.
    /// #   let worksheet = workbook.add_worksheet();
    /// #
    ///     // Write some strings to the worksheet.
    ///     worksheet.write_string(0,  0, "السلام عليكم")?;
    ///     worksheet.write_string(1,  0, "Dobrý den")?;
    ///     worksheet.write_string(2,  0, "Hello")?;
    ///     worksheet.write_string(3,  0, "שָׁלוֹם")?;
    ///     worksheet.write_string(4,  0, "नमस्ते")?;
    ///     worksheet.write_string(5,  0, "こんにちは")?;
    ///     worksheet.write_string(6,  0, "안녕하세요")?;
    ///     worksheet.write_string(7,  0, "你好")?;
    ///     worksheet.write_string(8,  0, "Olá")?;
    ///     worksheet.write_string(9,  0, "Здравствуйте")?;
    ///     worksheet.write_string(10, 0, "Hola")?;
    /// #
    /// #     workbook.save("strings.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_string.png">
    ///
    pub fn write_string(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_string(row, col, string, None)
    }

    /// Write a formatted string to a worksheet cell.
    ///
    /// Write a string with formatting to a worksheet cell. The format is set
    /// via a [`Format`] struct which can control the font or color or
    /// properties such as bold and italic.
    ///
    /// Excel only supports UTF-8 text in the xlsx file format. Any rust UTF-8
    /// encoded string can be written with this method. The maximum string
    /// size supported by Excel is 32,767 characters.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `string` - The string to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxStringLengthExceeded`] - String exceeds Excel's limit
    ///   of 32,767 characters.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting different formatting for
    /// numbers in an Excel worksheet.
    ///
    /// ```
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    ///     // Create a new Excel file object.
    ///     let mut workbook = Workbook::new();
    ///
    ///     // Create some formats to use in the worksheet.
    ///     let bold_format = Format::new().set_bold();
    ///     let italic_format = Format::new().set_italic();
    ///
    ///     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Write some strings with formatting.
    ///     worksheet.write_string_with_format(0, 0, "Hello",     &bold_format)?;
    ///     worksheet.write_string_with_format(1, 0, "שָׁלוֹם",      &bold_format)?;
    ///     worksheet.write_string_with_format(2, 0, "नमस्ते",      &italic_format)?;
    ///     worksheet.write_string_with_format(3, 0, "こんにちは", &italic_format)?;
    ///
    /// #     workbook.save("strings.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_string_with_format.png">
    ///
    pub fn write_string_with_format(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_string(row, col, string, Some(format))
    }

    /// Write a "rich" string with multiple formats to a worksheet cell.
    ///
    /// The `write_rich_string()` method is used to write strings with
    /// multiple font formats within the string. For example strings like "This
    /// is **bold** and this is *italic*". For strings with a single format you
    /// can use the more common [`write_string_with_format()`](Worksheet::write_string)
    /// method.
    ///
    /// The basic rule is to break the string into pairs of [`Format`] and
    /// [`str`] fragments. So if we look at the above string again:
    ///
    /// * This is **bold** and this is *italic*
    ///
    /// The would be broken down into 4 fragments:
    ///
    /// ```text
    ///      default: |This is |
    ///      bold:    |bold|
    ///      default: | and this is |
    ///      italic:  |italic|
    /// ```
    ///
    /// This should then be converted to an array of [`Format`] and [`str`]
    /// tuples:
    ///
    /// ```text
    ///     let segments = [
    ///        (&default, "This is "),
    ///        (&red,     "red"),
    ///        (&default, " and this is "),
    ///        (&blue,    "blue"),
    ///     ];
    /// ```
    ///
    /// See the full example below.
    ///
    /// For the default format segments you can use
    /// [`Format::default()`](Format::default).
    ///
    /// Note, only the Font elements of the [`Format`] are used by Excel in rich
    /// strings. For example it isn't possible in Excel to highlight part of the
    /// string with a yellow background. It is possible to have a yellow
    /// background for the entire cell or to format other cell properties using
    /// an additional [`Format`] object and the
    /// [`write_rich_string_with_format()`](Worksheet::write_rich_string) method, see below.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `rich_string` - An array reference of `(&Format, &str)` tuples. See
    ///   the Errors section below for the restrictions.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxStringLengthExceeded`] - String exceeds Excel's limit
    ///   of 32,767 characters.
    /// * [`XlsxError::ParameterError`] - The following error cases will raise a
    ///   `ParameterError` error:
    ///   * If any of the str elements is empty. Excel doesn't allow this.
    ///   * If there isn't at least one `(&Format, &str)` tuple element in the
    ///     `rich_string` parameter array. Strictly speaking there should be at
    ///     least 2 tuples to make a rich string, otherwise it is just a normal
    ///     formatted string. However, Excel allows it.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing a "rich" string with multiple
    /// formats.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_rich_string.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     worksheet.set_column_width(0, 30)?;
    /// #
    ///     // Add some formats to use in the rich strings.
    ///     let default = Format::default();
    ///     let red = Format::new().set_font_color(XlsxColor::Red);
    ///     let blue = Format::new().set_font_color(XlsxColor::Blue);
    ///
    ///     // Write a Rich strings with multiple formats.
    ///     let segments = [
    ///         (&default, "This is "),
    ///         (&red,     "red"),
    ///         (&default, " and this is "),
    ///         (&blue,    "blue"),
    ///     ];
    ///     worksheet.write_rich_string(0, 0, &segments)?;
    ///
    ///     // It is possible, and idiomatic, to use slices as the string segments.
    ///     let text = "This is blue and this is red";
    ///     let segments = [
    ///         (&default, &text[..8]),
    ///         (&blue,    &text[8..12]),
    ///         (&default, &text[12..25]),
    ///         (&red,     &text[25..]),
    ///     ];
    ///     worksheet.write_rich_string(1, 0, &segments)?;
    ///
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_rich_string.png">
    ///
    pub fn write_rich_string(
        &mut self,
        row: RowNum,
        col: ColNum,
        rich_string: &[(&Format, &str)],
    ) -> Result<&mut Worksheet, XlsxError> {
        let (string, raw_string) = self.get_rich_string(rich_string)?;

        self.store_rich_string(row, col, &string, &raw_string, None)
    }

    /// Write a "rich" string with multiple formats to a worksheet cell, with an
    /// additional cell format.
    ///
    /// The `write_rich_string_with_format()` method is used to write strings with multiple
    /// font formats within the string. For example strings like "This is
    /// **bold** and this is *italic*". It also allows you to add an additional
    /// [`Format`] to the cell so that you can, for example, center the text in
    /// the cell.
    ///
    /// The syntax for creating and using `(&Format, &str)` tuples to create the
    /// rich string is shown above in
    /// [`write_rich_string()`](Worksheet::write_rich_string).
    ///
    /// For strings with a single format you can use the more common
    /// [`write_string_with_format()`](Worksheet::write_string) method.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `rich_string` - An array reference of `(&Format, &str)` tuples. See
    ///   the Errors section below for the restrictions.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxStringLengthExceeded`] - String exceeds Excel's limit
    ///   of 32,767 characters.
    /// * [`XlsxError::ParameterError`] - The following error cases will raise a
    ///   `ParameterError` error:
    ///   * If any of the str elements is empty. Excel doesn't allow this.
    ///   * If there isn't at least one `(&Format, &str)` tuple element in the
    ///     `rich_string` parameter array. Strictly speaking there should be at
    ///     least 2 tuples to make a rich string, otherwise it is just a normal
    ///     formatted string. However, Excel allows it.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing a "rich" string with multiple
    /// formats, and an additional cell format.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_rich_string_with_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, FormatAlign, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     worksheet.set_column_width(0, 30)?;
    /// #
    ///     // Add some formats to use in the rich strings.
    ///     let default = Format::default();
    ///     let red = Format::new().set_font_color(XlsxColor::Red);
    ///     let blue = Format::new().set_font_color(XlsxColor::Blue);
    ///
    ///     // Write a rich strings with multiple formats.
    ///     let segments = [
    ///         (&default, "This is "),
    ///         (&red,     "red"),
    ///         (&default, " and this is "),
    ///         (&blue,    "blue"),
    ///     ];
    ///     worksheet.write_rich_string(0, 0, &segments)?;
    ///
    ///     // Add an extra format to use for the entire cell.
    ///     let center = Format::new().set_align(FormatAlign::Center);
    ///
    ///     // Write the rich string again with the cell format.
    ///     worksheet.write_rich_string_with_format(2, 0, &segments, &center)?;
    ///
    ///
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_rich_string_with_format.png">
    ///
    pub fn write_rich_string_with_format(
        &mut self,
        row: RowNum,
        col: ColNum,
        rich_string: &[(&Format, &str)],
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        let (string, raw_string) = self.get_rich_string(rich_string)?;

        self.store_rich_string(row, col, &string, &raw_string, Some(format))
    }

    /// Write an unformatted formula to a worksheet cell.
    ///
    /// Write an unformatted Excel formula to a worksheet cell. See also the
    /// `rust_xlsxwriter` documentation on [Working with Formulas].
    ///
    /// [Working with Formulas]:
    ///     https://rustxlsxwriter.github.io/formulas/intro.html
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `formula` - The formula to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing formulas with formatting to a
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_formula.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Write some formulas to the worksheet.
    ///     worksheet.write_formula(0, 0, "=B3 + B4")?;
    ///     worksheet.write_formula(1, 0, "=SIN(PI()/4)")?;
    ///     worksheet.write_formula(2, 0, "=SUM(B1:B5)")?;
    ///     worksheet.write_formula(3, 0, r#"=IF(A3>1,"Yes", "No")"#)?;
    ///     worksheet.write_formula(4, 0, "=AVERAGE(1, 2, 3, 4)")?;
    ///     worksheet.write_formula(5, 0, r#"=DATEVALUE("1-Jan-2023")"#)?;
    ///
    /// #     workbook.save("formulas.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_formula.png">
    ///
    pub fn write_formula(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_formula(row, col, formula, None)
    }

    /// Write a formatted formula to a worksheet cell.
    ///
    /// Write a formula with formatting to a worksheet cell. The format is set
    /// via a [`Format`] struct which can control the font or color or
    /// properties such as bold and italic.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Working with Formulas].
    ///
    /// [Working with Formulas]: https://rustxlsxwriter.github.io/formulas/intro.html
    ///
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `formula` - The formula to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing formulas with formatting to a
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_formula_with_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     // Create some formats to use in the worksheet.
    ///     let bold_format = Format::new().set_bold();
    ///     let italic_format = Format::new().set_italic();
    ///
    ///     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Write some formulas with formatting.
    ///     worksheet.write_formula_with_format(0, 0, "=1+2+3", &bold_format)?;
    ///     worksheet.write_formula_with_format(1, 0, "=A1*2", &bold_format)?;
    ///     worksheet.write_formula_with_format(2, 0, "=SIN(PI()/4)", &italic_format)?;
    ///     worksheet.write_formula_with_format(3, 0, "=AVERAGE(1, 2, 3, 4)", &italic_format)?;
    ///
    /// #     workbook.save("formulas.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_formula_with_format.png">
    ///
    pub fn write_formula_with_format(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_formula(row, col, formula, Some(format))
    }

    /// Write an  array formula to a worksheet cell.
    ///
    /// The `write_array_formula()` method writes an array formula to a
    /// cell range. In Excel an array formula is a formula that performs a
    /// calculation on a range of values. It can return a single value or a
    /// range/"array" of values.
    ///
    /// An array formula is displayed with a pair of curly brackets around the
    /// formula like this: `{=SUM(A1:B1*A2:B2)}`. The `write_array()`
    /// method doesn't require actually require these so you can omit them in
    /// the formula, and the equal sign, if you wish like this:
    /// `SUM(A1:B1*A2:B2)`.
    ///
    /// For array formulas that return a range of values you must specify the
    /// range that the return values will be written to with the `first_` and
    /// `last_` parameters. If the array formula returns a single value then the
    /// first_ and last_ parameters should be the same, as shown in the example
    /// below.
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    /// * `formula` - The formula to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row or column is larger
    ///   than the last row or column.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing an array formulas to a
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_array_formula.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #    let worksheet = workbook.add_worksheet();
    /// #
    /// #    // Write some test data.
    /// #    worksheet.write_number(0, 1, 500)?;
    /// #    worksheet.write_number(0, 2, 300)?;
    /// #    worksheet.write_number(1, 1, 10)?;
    /// #    worksheet.write_number(1, 2, 15)?;
    /// #
    ///     // Write an array formula that returns a single value.
    ///     worksheet.write_array_formula(0, 0, 0, 0, "{=SUM(B1:C1*B2:C2)}")?;
    ///
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_array_formula.png">
    ///
    pub fn write_array_formula(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        formula: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_array_formula(
            first_row, first_col, last_row, last_col, formula, None, false,
        )
    }

    /// Write a formatted array formula to a worksheet cell.
    ///
    /// Write an array formula with formatting to a worksheet cell. The format
    /// is set via a [`Format`] struct which can control the font or color or
    /// properties such as bold and italic.
    ///
    /// The `write_array()` method writes an array formula to a cell
    /// range. In Excel an array formula is a formula that performs a
    /// calculation on a range of values. It can return a single value or a
    /// range/"array" of values.
    ///
    /// An array formula is displayed with a pair of curly brackets around the
    /// formula like this: `{=SUM(A1:B1*A2:B2)}`. The `write_array()`
    /// method doesn't require actually require these so you can omit them in
    /// the formula, and the equal sign, if you wish like this:
    /// `SUM(A1:B1*A2:B2)`.
    ///
    /// For array formulas that return a range of values you must specify the
    /// range that the return values will be written to with the `first_` and
    /// `last_` parameters. If the array formula returns a single value then the
    /// first_ and last_ parameters should be the same, as shown in the example
    /// below.
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    /// * `formula` - The formula to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing an array formula with
    /// formatting to a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_array_formula_with_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #    let worksheet = workbook.add_worksheet();
    /// #
    /// #    // Add a format.
    /// #    let bold = Format::new().set_bold();
    /// #
    /// #    // Write some test data.
    /// #    worksheet.write_number(0, 1, 500)?;
    /// #    worksheet.write_number(0, 2, 300)?;
    /// #    worksheet.write_number(1, 1, 10)?;
    /// #    worksheet.write_number(1, 2, 15)?;
    /// #
    ///     // Write an array formula that returns a single value.
    ///     worksheet.write_array_formula_with_format(0, 0, 0, 0, "{=SUM(B1:C1*B2:C2)}", &bold)?;
    ///
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_array_formula_with_format.png">
    ///
    pub fn write_array_formula_with_format(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        formula: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_array_formula(
            first_row,
            first_col,
            last_row,
            last_col,
            formula,
            Some(format),
            false,
        )
    }

    /// Write a dynamic array formula to a worksheet cell or range of cells.
    ///
    /// The `write_dynamic_array_formula()` function writes an Excel 365
    /// dynamic array formula to a cell range. Some examples of functions that
    /// return dynamic arrays are:
    ///
    /// - `FILTER()`
    /// - `RANDARRAY()`
    /// - `SEQUENCE()`
    /// - `SORTBY()`
    /// - `SORT()`
    /// - `UNIQUE()`
    /// - `XLOOKUP()`
    /// - `XMATCH()`
    ///
    /// For more details see the `rust_xlsxwriter` documentation section on
    /// [Dynamic Array support] and the [Dynamic array formulas] example.
    ///
    /// [Dynamic Array support]:
    ///     https://rustxlsxwriter.github.io/formulas/dynamic_arrays.html
    /// [Dynamic array formulas]:
    ///     https://rustxlsxwriter.github.io/examples/dynamic_arrays.html
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    /// * `formula` - The formula to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    ///
    /// # Examples
    ///
    /// The following example demonstrates a static function which generally
    /// returns one value turned into a dynamic array function which returns a
    /// range of values.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_dynamic_array_formula.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     // Write a dynamic formula using a static function.
    ///     worksheet.write_dynamic_array_formula(0, 1, 0, 1, "=LEN(A1:A3)")?;
    /// #
    /// #     // Write some data for the function to operate on.
    /// #     worksheet.write_string(0, 0, "Foo")?;
    /// #     worksheet.write_string(1, 0, "Food")?;
    /// #     worksheet.write_string(2, 0, "Frood")?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_dynamic_array_formula.png">
    ///
    pub fn write_dynamic_array_formula(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        formula: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_array_formula(
            first_row, first_col, last_row, last_col, formula, None, true,
        )
    }

    /// Write a formatted dynamic array formula to a worksheet cell or range of
    /// cells.
    ///
    /// The `write_dynamic_array_formula_with_format()` function writes an Excel 365 dynamic
    /// array formula to a cell range. Some examples of functions that return
    /// dynamic arrays are:
    ///
    /// - `FILTER()`
    /// - `RANDARRAY()`
    /// - `SEQUENCE()`
    /// - `SORTBY()`
    /// - `SORT()`
    /// - `UNIQUE()`
    /// - `XLOOKUP()`
    /// - `XMATCH()`
    ///
    /// The format is set via a [`Format`] struct which can control the font or
    /// color or properties such as bold and italic.
    ///
    /// For array formulas that return a range of values you must specify the
    /// range that the return values will be written to with the `first_` and
    /// `last_` parameters. If the array formula returns a single value then the
    /// first_ and last_ parameters should be the same, as shown in the example
    /// below or use the
    /// [`write_dynamic_formula_with_format()`](Worksheet::write_dynamic_formula_with_format()) method.
    ///
    /// For more details see the `rust_xlsxwriter` documentation section on
    /// [Dynamic Array support] and the [Dynamic array formulas] example.
    ///
    /// [Dynamic Array support]:
    ///     https://rustxlsxwriter.github.io/formulas/dynamic_arrays.html
    /// [Dynamic array formulas]:
    ///     https://rustxlsxwriter.github.io/examples/dynamic_arrays.html
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    /// * `formula` - The formula to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row or column is larger
    ///   than the last row or column.
    ///
    /// # Examples
    ///
    /// The following example demonstrates a static function which generally
    /// returns one value turned into a dynamic array function which returns a
    /// range of values.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_dynamic_array_formula_with_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     let bold = Format::new().set_bold();
    /// #
    /// #     // Write a dynamic formula using a static function.
    ///     worksheet.write_dynamic_array_formula_with_format(0, 1, 0, 1, "=LEN(A1:A3)", &bold)?;
    /// #
    /// #     // Write some data for the function to operate on.
    /// #     worksheet.write_string(0, 0, "Foo")?;
    /// #     worksheet.write_string(1, 0, "Food")?;
    /// #     worksheet.write_string(2, 0, "Frood")?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_dynamic_array_formula_with_format.png">
    ///
    pub fn write_dynamic_array_formula_with_format(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        formula: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_array_formula(
            first_row,
            first_col,
            last_row,
            last_col,
            formula,
            Some(format),
            true,
        )
    }

    /// Write a dynamic formula to a worksheet cell.
    ///
    /// The `write_dynamic_formula()` method is similar to the
    /// [`write_dynamic_array_formula()`](Worksheet::write_dynamic_array_formula())
    /// method, shown above, except that it writes a dynamic array formula to a
    /// single cell, rather than a range. This is a syntactic shortcut since the
    /// array range isn't generally known for a dynamic range and specifying the
    /// initial cell is sufficient for Excel.
    ///
    /// For more details see the `rust_xlsxwriter` documentation section on
    /// [Dynamic Array support] and the [Dynamic array formulas] example.
    ///
    /// [Dynamic Array support]:
    ///     https://rustxlsxwriter.github.io/formulas/dynamic_arrays.html
    /// [Dynamic array formulas]:
    ///     https://rustxlsxwriter.github.io/examples/dynamic_arrays.html
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `formula` - The formula to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    pub fn write_dynamic_formula(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_array_formula(row, col, row, col, formula, None, true)
    }

    /// Write a formatted dynamic formula to a worksheet cell.
    ///
    /// The `write_dynamic_formula_with_format()` method is similar to the
    /// [`write_dynamic_array_formula_with_format()`](Worksheet::write_dynamic_array_formula_with_format())
    /// method, shown above, except that it writes a dynamic array formula to a
    /// single cell, rather than a range. This is a syntactic shortcut since the
    /// array range isn't generally known for a dynamic range and specifying the
    /// initial cell is sufficient for Excel.
    ///
    /// For more details see the `rust_xlsxwriter` documentation section on
    /// [Dynamic Array support] and the [Dynamic array formulas] example.
    ///
    /// [Dynamic Array support]:
    ///     https://rustxlsxwriter.github.io/formulas/dynamic_arrays.html
    /// [Dynamic array formulas]:
    ///     https://rustxlsxwriter.github.io/examples/dynamic_arrays.html
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `formula` - The formula to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    pub fn write_dynamic_formula_with_format(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_array_formula(row, col, row, col, formula, Some(format), true)
    }

    /// Write a blank formatted worksheet cell.
    ///
    /// Write a blank cell with formatting to a worksheet cell. The format is
    /// set via a [`Format`] struct.
    ///
    /// Excel differentiates between an “Empty” cell and a “Blank” cell. An
    /// “Empty” cell is a cell which doesn’t contain data or formatting whilst a
    /// “Blank” cell doesn’t contain data but does contain formatting. Excel
    /// stores “Blank” cells but ignores “Empty” cells.
    ///
    /// The most common case for a formatted blank cell is to write a background
    /// or a border, see the example below.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing a blank cell with formatting,
    /// i.e., a cell that has no data but does have formatting.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_blank.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, FormatBorder, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     let format1 = Format::new().set_background_color(XlsxColor::Yellow);
    ///
    ///     let format2 = Format::new()
    ///         .set_background_color(XlsxColor::Yellow)
    ///         .set_border(FormatBorder::Thin);
    ///
    ///     worksheet.write_blank(1, 1, &format1)?;
    ///     worksheet.write_blank(3, 1, &format2)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_write_blank.png">
    ///
    pub fn write_blank(
        &mut self,
        row: RowNum,
        col: ColNum,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_blank(row, col, format)
    }

    /// Write a url/hyperlink to a worksheet cell.
    ///
    /// Write a url/hyperlink to a worksheet cell with the default Excel
    /// "Hyperlink" cell style.
    ///
    /// There are 3 types of url/link supported by Excel:
    ///
    /// 1. Web based URIs like:
    ///
    ///    * `http://`, `https://`, `ftp://`, `ftps://` and `mailto:`.
    ///
    /// 2. Local file links using the `file://` URI.
    ///
    ///    * `file:///Book2.xlsx`
    ///    * `file:///..\Sales\Book2.xlsx`
    ///    * `file:///C:\Temp\Book1.xlsx`
    ///    * `file:///Book2.xlsx#Sheet1!A1`
    ///    * `file:///Book2.xlsx#'Sales Data'!A1:G5`
    ///
    ///    Most paths will be relative to the root folder, following the Windows
    ///    convention, so most paths should start with `file:///`. For links to
    ///    other Excel files the url string can include a sheet and cell
    ///    reference after the `"#"` anchor, as shown in the last 2 examples
    ///    above. When using Windows paths, like in the examples above, it is
    ///    best to use a Rust raw string to avoid issues with the backslashes:
    ///    `r"file:///C:\Temp\Book1.xlsx"`.
    ///
    /// 3. Internal links to a cell or range of cells in the workbook using the
    ///    pseudo-uri `internal:`:
    ///
    ///    * `internal:Sheet2!A1`
    ///    * `internal:Sheet2!A1:G5`
    ///    * `internal:'Sales Data'!A1`
    ///
    ///    Worksheet references are typically of the form `Sheet1!A1` where a
    ///    worksheet and target cell should be specified. You can also link to a
    ///    worksheet range using the standard Excel range notation like
    ///    `Sheet1!A1:B2`. Excel requires that worksheet names containing spaces
    ///    or non alphanumeric characters are single quoted as follows `'Sales
    ///    Data'!A1`.
    ///
    /// The function will escape the following characters in URLs as required by
    /// Excel, ``\s " < > \ [ ] ` ^ { }``, unless the URL already contains `%xx`
    /// style escapes. In which case it is assumed that the URL was escaped
    /// correctly by the user and will by passed directly to Excel.
    ///
    /// Excel has a limit of around 2080 characters in the url string. Strings
    /// beyond this limit will raise an error, see below.
    ///
    /// For other variants of this function see:
    ///
    /// * [`write_url_with_text()`](Worksheet::write_url_with_text()) to add
    ///   alternative text to the link.
    /// * [`write_url_with_format()`](Worksheet::write_url_with_format()) to add
    ///   an alternative format to the link.
    /// * [`write_url_with_options()`](Worksheet::write_url_with_options()) to
    ///   add a screen tip and all other options to the link.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `string` - The url string to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxUrlLengthExceeded`] - URL string or anchor exceeds
    ///   Excel's limit of 2080 characters.
    /// * [`XlsxError::UnknownUrlType`] - The URL has an unknown URI type. See
    ///   the supported types listed above.
    ///
    /// # Examples
    ///
    /// The following example demonstrates several of the url writing methods.
    ///
    /// ```
    /// # // This code is available in examples/app_hyperlinks.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxColor, XlsxError, FormatUnderline};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Create a format to use in the worksheet.
    /// #     let link_format = Format::new()
    /// #         .set_font_color(XlsxColor::Red)
    /// #         .set_underline(FormatUnderline::Single);
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet1 = workbook.add_worksheet();
    /// #
    /// #     // Set the column width for clarity.
    /// #     worksheet1.set_column_width(0, 26)?;
    /// #
    ///     // Write some url links.
    ///     worksheet1.write_url(0, 0, "https://www.rust-lang.org")?;
    ///     worksheet1.write_url_with_text(1, 0, "https://www.rust-lang.org", "Learn Rust")?;
    ///     worksheet1.write_url_with_format(2, 0, "https://www.rust-lang.org", &link_format)?;
    ///
    ///     // Write some internal links.
    ///     worksheet1.write_url(4, 0, "internal:Sheet1!A1")?;
    ///     worksheet1.write_url(5, 0, "internal:Sheet2!C4")?;
    ///
    ///     // Write some external links.
    ///     worksheet1.write_url(7, 0, r"file:///C:\Temp\Book1.xlsx")?;
    ///     worksheet1.write_url(8, 0, r"file:///C:\Temp\Book1.xlsx#Sheet1!C4")?;
    ///
    ///     // Add another sheet to link to.
    ///     let worksheet2 = workbook.add_worksheet();
    ///     worksheet2.write_string(3, 2, "Here I am")?;
    ///     worksheet2.write_url_with_text(4, 2, "internal:Sheet1!A6", "Go back")?;
    ///
    /// #     // Save the file to disk.
    /// #     workbook.save("hyperlinks.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/app_hyperlinks.png">
    ///
    pub fn write_url(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_url(row, col, string, "", "", None)
    }

    /// Write a url/hyperlink to a worksheet cell with an alternative text.
    ///
    /// Write a url/hyperlink to a worksheet cell with an alternative, user
    /// friendly, text and the default Excel "Hyperlink" cell style.
    ///
    /// This method is similar to [`write_url()`](Worksheet::write_url())  except
    /// that you can specify an alternative string for the url. For example you
    /// could have a cell contain the link [Learn
    /// Rust](https://www.rust-lang.org) instead of the raw link
    /// <https://www.rust-lang.org>.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `string` - The url string to write to the cell.
    /// * `text` - The alternative string to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxStringLengthExceeded`] - Text string exceeds Excel's
    ///   limit of 32,767 characters.
    /// * [`XlsxError::MaxUrlLengthExceeded`] - URL string or anchor exceeds
    ///   Excel's limit of 2080 characters.
    /// * [`XlsxError::UnknownUrlType`] - The URL has an unknown URI type. See
    ///   the supported types listed above.
    ///
    /// # Examples
    ///
    /// A simple, getting started, example of some of the features of the
    /// rust_xlsxwriter library.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_url_with_text.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook , XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Write a url and alternative text.
    ///     worksheet.write_url_with_text(0, 0, "https://www.rust-lang.org", "Learn Rust")?;
    /// #
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_url_with_text.png">
    ///
    pub fn write_url_with_text(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
        text: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_url(row, col, string, text, "", None)
    }

    /// Write a url/hyperlink to a worksheet cell with a user defined format
    ///
    /// Write a url/hyperlink to a worksheet cell with a user defined format
    /// instead of the default Excel "Hyperlink" cell style.
    ///
    /// This method is similar to [`write_url()`](Worksheet::write_url())
    /// except that you can specify an alternative format for the url.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `string` - The url string to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxUrlLengthExceeded`] - URL string or anchor exceeds
    ///   Excel's limit of 2080 characters.
    /// * [`XlsxError::UnknownUrlType`] - The URL has an unknown URI type. See
    ///   the supported types listed above.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing a url with alternative format.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_url_with_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxColor, XlsxError, FormatUnderline};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Create a format to use in the worksheet.
    ///     let link_format = Format::new()
    ///         .set_font_color(XlsxColor::Red)
    ///         .set_underline(FormatUnderline::Single);
    ///
    ///     // Write a url with an alternative format.
    ///     worksheet.write_url_with_format(0, 0, "https://www.rust-lang.org", &link_format)?;
    ///
    /// #    // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_url_with_format.png">
    ///
    pub fn write_url_with_format(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_url(row, col, string, "", "", Some(format))
    }

    /// Write a url/hyperlink to a worksheet cell with various options
    ///
    /// This method is similar to [`write_url()`](Worksheet::write_url()) and
    /// variant methods except that you can also add a screen tip message, if
    /// required.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `string` - The url string to write to the cell.
    /// * `text` - The alternative string to write to the cell.
    /// * `tip` - The screen tip string to display when the user hovers over the
    ///   url cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// The `text` and `tip` parameters are optional and can be set as a blank
    /// string. The `format` is an `Option<>` parameter and can be specified as `None` if not required.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::MaxStringLengthExceeded`] - Text string exceeds Excel's
    ///   limit of 32,767 characters.
    /// * [`XlsxError::MaxUrlLengthExceeded`] - URL string or anchor exceeds
    ///   Excel's limit of 2080 characters or the screen tip exceed 255 characters.
    /// * [`XlsxError::UnknownUrlType`] - The URL has an unknown URI type. See
    ///   the supported types listed above.
    ///
    pub fn write_url_with_options(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
        text: &str,
        tip: &str,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_url(row, col, string, text, tip, format)
    }

    /// Write a formatted date and time to a worksheet cell.
    ///
    /// Write a [`chrono::NaiveDateTime`] instance as an Excel datetime to a
    /// worksheet cell. The [chrono] framework provides a comprehensive range of
    /// functions and types for dealing with times and dates. The serial
    /// dates/times used by Excel don't support timezones so the `Naive` chrono
    /// variants are used.
    ///
    /// Excel stores dates and times as a floating point number with a number
    /// format to defined how it is displayed. The number format is set via a
    /// [`Format`] struct which can also control visual formatting such as bold
    /// and italic text.
    ///
    /// [`chrono::NaiveDateTime`]:
    ///     https://docs.rs/chrono/latest/chrono/naive/struct.NaiveDateTime.html
    ///
    /// [chrono]: https://docs.rs/chrono/latest/chrono/index.html
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `datetime` - A [`chrono::NaiveDateTime`] instance.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing formatted datetimes in an
    /// Excel worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_datetime.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// # use chrono::NaiveDate;
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Create some formats to use with the datetimes below.
    ///     let format1 = Format::new().set_num_format("dd/mm/yyyy hh::mm");
    ///     let format2 = Format::new().set_num_format("mm/dd/yyyy hh::mm");
    ///     let format3 = Format::new().set_num_format("yyyy-mm-ddThh::mm:ss");
    ///     let format4 = Format::new().set_num_format("ddd dd mmm yyyy hh::mm");
    ///     let format5 = Format::new().set_num_format("dddd, mmmm dd, yyyy hh::mm");
    ///
    ///     // Set the column width for clarity.
    ///     worksheet.set_column_width(0, 30)?;
    ///
    ///     // Create a datetime object.
    ///     let datetime = NaiveDate::from_ymd_opt(2023, 1, 25)
    ///         .unwrap()
    ///         .and_hms_opt(12, 30, 0)
    ///         .unwrap();
    ///
    ///     // Write the datetime with different Excel formats.
    ///     worksheet.write_datetime(0, 0, &datetime, &format1)?;
    ///     worksheet.write_datetime(1, 0, &datetime, &format2)?;
    ///     worksheet.write_datetime(2, 0, &datetime, &format3)?;
    ///     worksheet.write_datetime(3, 0, &datetime, &format4)?;
    ///     worksheet.write_datetime(4, 0, &datetime, &format5)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_datetime.png">
    ///
    pub fn write_datetime(
        &mut self,
        row: RowNum,
        col: ColNum,
        datetime: &NaiveDateTime,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        let number = self.datetime_to_excel(datetime);

        // Store the cell data.
        self.store_datetime(row, col, number, Some(format))
    }

    /// Write a formatted date to a worksheet cell.
    ///
    /// Write a [`chrono::NaiveDateTime`] instance as an Excel datetime to a
    /// worksheet cell. The [chrono] framework provides a comprehensive range of
    /// functions and types for dealing with times and dates. The serial
    /// dates/times used by Excel don't support timezones so the `Naive` chrono
    /// variants are used.
    ///
    /// Excel stores dates and times as a floating point number with a number
    /// format to defined how it is displayed. The number format is set via a
    /// [`Format`] struct which can also control visual formatting such as bold
    /// and italic text.
    ///
    /// [`chrono::NaiveDate`]:
    ///     https://docs.rs/chrono/latest/chrono/naive/struct.NaiveDate.html
    ///
    /// [chrono]: https://docs.rs/chrono/latest/chrono/index.html
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `date` - A [`chrono::NaiveDate`] instance.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing formatted dates in an Excel
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_date.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// # use chrono::NaiveDate;
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Create some formats to use with the dates below.
    ///     let format1 = Format::new().set_num_format("dd/mm/yyyy");
    ///     let format2 = Format::new().set_num_format("mm/dd/yyyy");
    ///     let format3 = Format::new().set_num_format("yyyy-mm-dd");
    ///     let format4 = Format::new().set_num_format("ddd dd mmm yyyy");
    ///     let format5 = Format::new().set_num_format("dddd, mmmm dd, yyyy");
    ///
    ///     // Set the column width for clarity.
    ///     worksheet.set_column_width(0, 30)?;
    ///
    ///     // Create a date object.
    ///     let date = NaiveDate::from_ymd_opt(2023, 1, 25).unwrap();
    ///
    ///     // Write the date with different Excel formats.
    ///     worksheet.write_date(0, 0, &date, &format1)?;
    ///     worksheet.write_date(1, 0, &date, &format2)?;
    ///     worksheet.write_date(2, 0, &date, &format3)?;
    ///     worksheet.write_date(3, 0, &date, &format4)?;
    ///     worksheet.write_date(4, 0, &date, &format5)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_date.png">
    ///
    pub fn write_date(
        &mut self,
        row: RowNum,
        col: ColNum,
        date: &NaiveDate,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        let number = self.date_to_excel(date);

        // Store the cell data.
        self.store_datetime(row, col, number, Some(format))
    }

    /// Write a formatted time to a worksheet cell.
    ///
    /// Write a [`chrono::NaiveDateTime`] instance as an Excel datetime to a
    /// worksheet cell. The [chrono] framework provides a comprehensive range of
    /// functions and types for dealing with times and dates. The serial
    /// dates/times used by Excel don't support timezones so the `Naive` chrono
    /// variants are used.
    ///
    /// Excel stores dates and times as a floating point number with a number
    /// format to defined how it is displayed. The number format is set via a
    /// [`Format`] struct which can also control visual formatting such as bold
    /// and italic text.
    ///
    /// [`chrono::NaiveTime`]:
    ///     https://docs.rs/chrono/latest/chrono/naive/struct.NaiveTime.html
    ///
    /// [chrono]: https://docs.rs/chrono/latest/chrono/index.html
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `time` - A [`chrono::NaiveTime`] instance.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing formatted times in an Excel
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_time.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// # use chrono::NaiveTime;
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Create some formats to use with the times below.
    ///     let format1 = Format::new().set_num_format("h::mm");
    ///     let format2 = Format::new().set_num_format("hh::mm");
    ///     let format3 = Format::new().set_num_format("hh::mm:ss");
    ///     let format4 = Format::new().set_num_format("hh::mm:ss.000");
    ///     let format5 = Format::new().set_num_format("h::mm AM/PM");
    ///
    ///     // Set the column width for clarity.
    ///     worksheet.set_column_width(0, 30)?;
    ///
    ///     // Create a time object.
    ///     let time = NaiveTime::from_hms_milli(2, 59, 3, 456);
    ///
    ///     // Write the time with different Excel formats.
    ///     worksheet.write_time(0, 0, &time, &format1)?;
    ///     worksheet.write_time(1, 0, &time, &format2)?;
    ///     worksheet.write_time(2, 0, &time, &format3)?;
    ///     worksheet.write_time(3, 0, &time, &format4)?;
    ///     worksheet.write_time(4, 0, &time, &format5)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_time.png">
    ///
    pub fn write_time(
        &mut self,
        row: RowNum,
        col: ColNum,
        time: &NaiveTime,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        let number = self.time_to_excel(time);

        // Store the cell data.
        self.store_datetime(row, col, number, Some(format))
    }

    /// Write an unformatted boolean value to a cell.
    ///
    /// Write an unformatted Excel boolean value to a worksheet cell.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `boolean` - The boolean value to write to the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing boolean values to a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_boolean.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     worksheet.write_boolean(0, 0, true)?;
    ///     worksheet.write_boolean(1, 0, false)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_boolean.png">
    ///
    pub fn write_boolean(
        &mut self,
        row: RowNum,
        col: ColNum,
        boolean: bool,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_boolean(row, col, boolean, None)
    }

    /// Write a formatted boolean value to a worksheet cell.
    ///
    /// Write a boolean value with formatting to a worksheet cell. The format is set
    /// via a [`Format`] struct which can control the numerical formatting of
    /// the number, for example as a currency or a percentage value, or the
    /// visual format, such as bold and italic text.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `boolean` - The boolean value to write to the cell.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing formatted boolean values to a
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_write_boolean_with_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     let bold = Format::new().set_bold();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     worksheet.write_boolean_with_format(0, 0, true, &bold)?;
    ///     worksheet.write_boolean_with_format(1, 0, false, &bold)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_write_boolean_with_format.png">
    ///
    ///
    pub fn write_boolean_with_format(
        &mut self,
        row: RowNum,
        col: ColNum,
        boolean: bool,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Store the cell data.
        self.store_boolean(row, col, boolean, Some(format))
    }

    /// Merge a range of cells.
    ///
    /// The `merge_range()` method allows cells to be merged together so that
    /// they act as a single area.
    ///
    /// The `merge_range()` method writes a string to the merged cells. In order
    /// to write other data types, such as a number or a formula, you can
    /// overwrite the first cell with a call to one of the other
    /// `worksheet.write_*()` functions. The same [`Format`] instance should be
    /// used as was used in the merged range, see the example below.
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    /// * `string` - The string to write to the cell. Other types can also be
    ///   handled. See the documentation above and the example below.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    /// * [`XlsxError::MergeRangeSingleCell`] - A merge range cannot be a single
    ///   cell in Excel.
    /// * [`XlsxError::MergeRangeOverlaps`] - The merge range overlaps a
    ///   previous merge range.
    ///
    ///
    /// # Examples
    ///
    /// An example of creating merged ranges in a worksheet using the
    /// rust_xlsxwriter library.
    ///
    /// ```
    /// # // This code is available in examples/app_merge_range.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, FormatAlign, FormatBorder, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Write some merged cells with centering.
    ///     let format = Format::new().set_align(FormatAlign::Center);
    ///
    ///     worksheet.merge_range(1, 1, 1, 2, "Merged cells", &format)?;
    ///
    ///     // Write some merged cells with centering and a border.
    ///     let format = Format::new()
    ///         .set_align(FormatAlign::Center)
    ///         .set_border(FormatBorder::Thin);
    ///
    ///     worksheet.merge_range(3, 1, 3, 2, "Merged cells", &format)?;
    ///
    ///     // Write some merged cells with a number by overwriting the first cell in
    ///     // the string merge range with the formatted number.
    ///     worksheet.merge_range(5, 1, 5, 2, "", &format)?;
    ///     worksheet.write_number_with_format(5, 1, 12345.67, &format)?;
    ///
    ///     // Example with a more complex format and larger range.
    ///     let format = Format::new()
    ///         .set_align(FormatAlign::Center)
    ///         .set_align(FormatAlign::VerticalCenter)
    ///         .set_border(FormatBorder::Thin)
    ///         .set_background_color(XlsxColor::Silver);
    ///
    ///     worksheet.merge_range(7, 1, 8, 3, "Merged cells", &format)?;
    ///
    /// #    // Save the file to disk.
    /// #     workbook.save("merge_range.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/app_merge_range.png">
    ///
    pub fn merge_range(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        string: &str,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows and cols are in the allowed range.
        if !self.check_dimensions(first_row, first_col)
            || !self.check_dimensions(last_row, last_col)
        {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_row > last_row || first_col > last_col {
            return Err(XlsxError::RowColumnOrderError);
        }

        // Check that the range isn't a singe cell, which isn't allowed by Excel.
        if first_row == last_row && first_col == last_col {
            return Err(XlsxError::MergeRangeSingleCell);
        }

        // Write the first cell in the range.
        self.write_string_with_format(first_row, first_col, string, format)?;

        // Pad out the rest of the range with formatted blanks cells.
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                // Skip the first cell which was written above.
                if row == first_row && col == first_col {
                    continue;
                }
                self.write_blank(row, col, format)?;
            }
        }

        // Create a cell range for storage/testing.
        let cell_range = CellRange {
            first_row,
            first_col,
            last_row,
            last_col,
        };

        // Check if the merged range overlaps any previous merged range. This is
        // a major error in Excel.
        let new_index = self.merged_ranges.len();
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                match self.merged_cells.get_mut(&(row, col)) {
                    Some(index) => {
                        let previous_cell_range = self.merged_ranges.get(*index).unwrap();
                        return Err(XlsxError::MergeRangeOverlaps(
                            cell_range.to_error_string(),
                            previous_cell_range.to_error_string(),
                        ));
                    }
                    None => self.merged_cells.insert((row, col), new_index),
                };
            }
        }

        // Store the merge range if everything was okay.
        self.merged_ranges.push(cell_range);

        Ok(self)
    }

    /// Add an image to a worksheet.
    ///
    /// Add an image to a worksheet at a cell location. The image should be
    /// encapsulated in an [`Image`] object.
    ///
    /// The supported image formats are:
    ///
    /// - PNG
    /// - JPG
    /// - GIF: The image can be an animated gif in more resent versions of
    ///   Excel.
    /// - BMP: BMP images are only supported for backward compatibility. In
    ///   general it is best to avoid BMP images since they are not compressed.
    ///   If used, BMP images must be 24 bit, true color, bitmaps.
    ///
    /// EMF and WMF file formats will be supported in an upcoming version of the
    /// library.
    ///
    /// **NOTE on SVG files**: Excel doesn't directly support SVG files in the
    /// same way as other image file formats. It allows SVG to be inserted into
    /// a worksheet but converts them to, and displays them as, PNG files. It
    /// stores the original SVG image in the file so the original format can be
    /// retrieved. This removes the file size and resolution advantage of using
    /// SVG files. As such SVG files are not supported by `rust_xlsxwriter`
    /// since a conversion to the PNG format would be required and that format
    /// is already supported.
    ///
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `image` - The [`Image`] to insert into the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates creating a new Image object and
    /// adding it to a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_image.rs
    /// #
    /// # use rust_xlsxwriter::{Image, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Create a new image object.
    ///     let image = Image::new("examples/rust_logo.png")?;
    ///
    ///     // Insert the image.
    ///     worksheet.insert_image(1, 2, &image)?;
    /// #
    /// #     // Save the file to disk.
    /// #     workbook.save("image.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/image_intro.png">
    ///
    pub fn insert_image(
        &mut self,
        row: RowNum,
        col: ColNum,
        image: &Image,
    ) -> Result<&mut Worksheet, XlsxError> {
        self.insert_image_with_offset(row, col, image, 0, 0)?;

        Ok(self)
    }

    /// Add an image to a worksheet at an offset.
    ///
    /// Add an image to a worksheet at a pixel offset within a cell location.
    /// The image should be encapsulated in an [`Image`] object.
    ///
    /// This method is similar to [`insert_image()`](Worksheet::insert_image)
    /// except that the image can be offset from the top left of the cell.
    ///
    /// Note, it is possible to offset the image outside the target cell if
    /// required.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `image` - The [`Image`] to insert into the cell.
    /// * `x_offset`: The horizontal offset within the cell in pixels.
    /// * `y_offset`: The vertical offset within the cell in pixels.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// This example shows how to add an image to a worksheet at an offset within
    /// the cell.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_insert_image_with_offset.rs
    /// #
    /// # use rust_xlsxwriter::{Image, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Create a new image object.
    ///     let image = Image::new("examples/rust_logo.png")?;
    ///
    ///     // Insert the image at an offset.
    ///     worksheet.insert_image_with_offset(1, 2, &image, 10, 5)?;
    ///
    /// #     // Save the file to disk.
    /// #     workbook.save("image.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_insert_image_with_offset.png">
    ///
    pub fn insert_image_with_offset(
        &mut self,
        row: RowNum,
        col: ColNum,
        image: &Image,
        x_offset: u32,
        y_offset: u32,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and columns are in the allowed range.
        if !self.check_dimensions_only(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        let mut image = image.clone();
        image.x_offset = x_offset;
        image.y_offset = y_offset;

        self.images.insert((row, col), image);

        Ok(self)
    }

    /// TODO
    pub fn insert_chart(
        &mut self,
        row: RowNum,
        col: ColNum,
        chart: &Chart,
    ) -> Result<&mut Worksheet, XlsxError> {
        self.insert_chart_with_offset(row, col, chart, 0, 0)?;

        Ok(self)
    }

    /// TODO
    ///
    pub fn insert_chart_with_offset(
        &mut self,
        row: RowNum,
        col: ColNum,
        chart: &Chart,
        x_offset: u32,
        y_offset: u32,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and columns are in the allowed range.
        if !self.check_dimensions_only(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        let mut chart = chart.clone();
        chart.x_offset = x_offset;
        chart.y_offset = y_offset;

        self.charts.insert((row, col), chart);

        Ok(self)
    }

    /// Set the height for a row of cells.
    ///
    /// The `set_row_height()` method is used to change the default height of a
    /// row. The height is specified in character units, where the default
    /// height is 15. Excel allows height values in increments of 0.25.
    ///
    /// To specify the height in pixels use the
    /// [`set_row_height_pixels()`](Worksheet::set_row_height_pixels()) method.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `height` - The row height in character units.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the height for a row in
    /// Excel.
    ///
    /// ```
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Add some text.
    ///     worksheet.write_string(0, 0, "Normal")?;
    ///     worksheet.write_string(2, 0, "Taller")?;
    ///
    ///     // Set the row height in Excel character units.
    ///     worksheet.set_row_height(2, 30)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_row_height.png">
    ///
    pub fn set_row_height<T>(&mut self, row: RowNum, height: T) -> Result<&mut Worksheet, XlsxError>
    where
        T: Into<f64>,
    {
        let height = height.into();

        // If the height is 0 then the Excel treats the row as hidden with
        // default height.
        if height == 0.0 {
            return self.set_row_hidden(row);
        }

        // Set a suitable column range for the row dimension check/set.
        let min_col = if self.dimensions.first_col != COL_MAX {
            self.dimensions.first_col
        } else {
            0
        };

        // Check row is in the allowed range.
        if !self.check_dimensions(row, min_col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Update an existing row metadata object or create a new one.
        match self.changed_rows.get_mut(&row) {
            Some(row_options) => row_options.height = height,
            None => {
                let row_options = RowOptions {
                    height,
                    xf_index: 0,
                    hidden: false,
                };
                self.changed_rows.insert(row, row_options);
            }
        }

        Ok(self)
    }

    /// Set the height for a row of cells, in pixels.
    ///
    /// The `set_row_height_pixels()` method is used to change the default height of a
    /// row. The height is specified in pixels, where the default
    /// height is 20.
    ///
    /// To specify the height in Excel's character units use the
    /// [`set_row_height()`](Worksheet::set_row_height()) method.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `height` - The row height in pixels.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the height for a row in Excel.
    ///
    /// ```
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Add some text.
    ///     worksheet.write_string(0, 0, "Normal")?;
    ///     worksheet.write_string(2, 0, "Taller")?;
    ///
    ///     // Set the row height in pixels.
    ///     worksheet.set_row_height_pixels(2, 40)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_row_height.png">
    ///
    pub fn set_row_height_pixels(
        &mut self,
        row: RowNum,
        height: u16,
    ) -> Result<&mut Worksheet, XlsxError> {
        let height = 0.75 * height as f64;

        self.set_row_height(row, height)
    }

    /// Set the format for a row of cells.
    ///
    /// The `set_row_format()` method is used to change the default format of a
    /// row. Any unformatted data written to that row will then adopt that
    /// format. Formatted data written to the row will maintain its own cell
    /// format. See the example below.
    ///
    /// A future version of this library may support automatic merging of
    /// explicit cell formatting with the row formatting but that isn't
    /// currently supported.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the format for a row in Excel.
    ///
    /// ```
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Add for formats.
    ///     let bold_format = Format::new().set_bold();
    ///     let red_format = Format::new().set_font_color(XlsxColor::Red);
    ///
    ///     // Set the row format.
    ///     worksheet.set_row_format(1, &red_format)?;
    ///
    ///     // Add some unformatted text that adopts the row format.
    ///     worksheet.write_string(1, 0, "Hello")?;
    ///
    ///     // Add some formatted text that overrides the row format.
    ///     worksheet.write_string_with_format(1, 2, "Hello", &bold_format)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_row_format.png">
    ///
    pub fn set_row_format(
        &mut self,
        row: RowNum,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Set a suitable column range for the row dimension check/set.
        let min_col = if self.dimensions.first_col != COL_MAX {
            self.dimensions.first_col
        } else {
            0
        };

        // Check row is in the allowed range.
        if !self.check_dimensions(row, min_col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Get the index of the format object.
        let xf_index = self.format_index(format);

        // Update an existing row metadata object or create a new one.
        match self.changed_rows.get_mut(&row) {
            Some(row_options) => row_options.xf_index = xf_index,
            None => {
                let row_options = RowOptions {
                    height: DEFAULT_ROW_HEIGHT,
                    xf_index,
                    hidden: false,
                };
                self.changed_rows.insert(row, row_options);
            }
        }

        Ok(self)
    }

    /// Hide a worksheet row.
    ///
    /// The `set_row_hidden()` method is used to hide a row. This can be
    /// used, for example, to hide intermediary steps in a complicated
    /// calculation.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates hiding a worksheet row.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_row_hidden.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Hide row 2 (with zero indexing).
    ///     worksheet.set_row_hidden(1)?;
    ///
    ///     worksheet.write_string(2, 0, "Row 2 is hidden")?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_row_hidden.png">
    ///
    pub fn set_row_hidden(&mut self, row: RowNum) -> Result<&mut Worksheet, XlsxError> {
        // Set a suitable column range for the row dimension check/set.
        let min_col = if self.dimensions.first_col != COL_MAX {
            self.dimensions.first_col
        } else {
            0
        };

        // Check row is in the allowed range.
        if !self.check_dimensions(row, min_col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Update an existing row metadata object or create a new one.
        match self.changed_rows.get_mut(&row) {
            Some(row_options) => row_options.hidden = true,
            None => {
                let row_options = RowOptions {
                    height: DEFAULT_ROW_HEIGHT,
                    xf_index: 0,
                    hidden: true,
                };
                self.changed_rows.insert(row, row_options);
            }
        }

        Ok(self)
    }

    /// Unhide a user hidden worksheet row.
    ///
    /// The `set_row_unhidden()` method is used to unhide a previously hidden
    /// row. This can occasionally be useful when used in conjunction with
    /// autofilter rules.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row exceeds Excel's worksheet
    ///   limits.
    ///
    pub fn set_row_unhidden(&mut self, row: RowNum) -> Result<&mut Worksheet, XlsxError> {
        // Set a suitable column range for the row dimension check/set.
        let min_col = if self.dimensions.first_col != COL_MAX {
            self.dimensions.first_col
        } else {
            0
        };

        // Check row is in the allowed range.
        if !self.check_dimensions(row, min_col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Only update an existing row metadata object.
        if let Some(row_options) = self.changed_rows.get_mut(&row) {
            row_options.hidden = false;
        }

        Ok(self)
    }

    /// Set the width for a worksheet column.
    ///
    /// The `set_column_width()` method is used to change the default width of a
    /// worksheet column.
    ///
    /// The ``width`` parameter sets the column width in the same units used by
    /// Excel which is: the number of characters in the default font. The
    /// default width is 8.43 in the default font of Calibri 11. The actual
    /// relationship between a string width and a column width in Excel is
    /// complex. See the [following explanation of column
    /// widths](https://support.microsoft.com/en-us/kb/214123) from the
    /// Microsoft support documentation for more details. To set the width in
    /// pixels use the
    /// [`set_column_width_pixels()`](Worksheet::set_column_width_pixels())
    /// method.
    ///
    /// See also the [`autofit()`](Worksheet::autofit()) method.
    ///
    /// # Arguments
    ///
    /// * `col` - The zero indexed column number.
    /// * `width` - The row width in character units.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Column exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the width of columns in
    /// Excel.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_column_width.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Add some text.
    ///     worksheet.write_string(0, 0, "Normal")?;
    ///     worksheet.write_string(0, 2, "Wider")?;
    ///     worksheet.write_string(0, 4, "Narrower")?;
    ///
    ///     // Set the column width in Excel character units.
    ///     worksheet.set_column_width(2, 16)?;
    ///     worksheet.set_column_width(4, 4)?;
    ///     worksheet.set_column_width(5, 4)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_column_width.png">
    ///
    pub fn set_column_width<T>(
        &mut self,
        col: ColNum,
        width: T,
    ) -> Result<&mut Worksheet, XlsxError>
    where
        T: Into<f64>,
    {
        let width = width.into();

        // If the width is 0 then the Excel treats the column as hidden with
        // default width.
        if width == 0.0 {
            return self.set_column_hidden(col);
        }

        // Check if column is in the allowed range without updating dimensions.
        if col >= COL_MAX {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Store the column width.
        self.store_column_width(col, width, false);

        Ok(self)
    }

    /// Set the width for a worksheet column in pixels.
    ///
    /// The `set_column_width()` method is used to change the default width of a
    /// worksheet column.
    ///
    /// To set the width in Excel character units use the
    /// [`set_column_width()`](Worksheet::set_column_width()) method.
    ///
    /// See also the [`autofit()`](Worksheet::autofit()) method.
    ///
    /// # Arguments
    ///
    /// * `col` - The zero indexed column number.
    /// * `width` - The row width in pixels.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Column exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the width of columns in Excel
    /// in pixels.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_column_width_pixels.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Add some text.
    ///     worksheet.write_string(0, 0, "Normal")?;
    ///     worksheet.write_string(0, 2, "Wider")?;
    ///     worksheet.write_string(0, 4, "Narrower")?;
    ///
    ///     // Set the column width in pixels.
    ///     worksheet.set_column_width_pixels(2, 117)?;
    ///     worksheet.set_column_width_pixels(4, 33)?;
    ///     worksheet.set_column_width_pixels(5, 33)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_column_width.png">
    ///
    pub fn set_column_width_pixels(
        &mut self,
        col: ColNum,
        width: u16,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Properties for Calibri 11.
        let max_digit_width = 7.0_f64;
        let padding = 5.0_f64;
        let mut width = width as f64;

        if width < 12.0 {
            width /= max_digit_width + padding;
        } else {
            width = (width - padding) / max_digit_width
        }

        self.set_column_width(col, width)
    }

    /// Set the format for a column of cells.
    ///
    /// The `set_column_format()` method is used to change the default format of a
    /// column. Any unformatted data written to that column will then adopt that
    /// format. Formatted data written to the column will maintain its own cell
    /// format. See the example below.
    ///
    /// A future version of this library may support automatic merging of
    /// explicit cell formatting with the column formatting but that isn't
    /// currently supported.
    ///
    /// # Arguments
    ///
    /// * `col` - The zero indexed column number.
    /// * `format` - The [`Format`] property for the cell.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Column exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the format for a column in Excel.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_column_format.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Add for formats.
    ///     let bold_format = Format::new().set_bold();
    ///     let red_format = Format::new().set_font_color(XlsxColor::Red);
    ///
    ///     // Set the column format.
    ///     worksheet.set_column_format(1, &red_format)?;
    ///
    ///     // Add some unformatted text that adopts the column format.
    ///     worksheet.write_string(0, 1, "Hello")?;
    ///
    ///     // Add some formatted text that overrides the column format.
    ///     worksheet.write_string_with_format(2, 1, "Hello", &bold_format)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_column_format.png">
    ///
    pub fn set_column_format(
        &mut self,
        col: ColNum,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Set a suitable row range for the dimension check/set.
        let min_row = if self.dimensions.first_row != ROW_MAX {
            self.dimensions.first_row
        } else {
            0
        };

        // Check column is in the allowed range.
        if !self.check_dimensions(min_row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Get the index of the format object.
        let xf_index = self.format_index(format);

        // Update an existing col metadata object or create a new one.
        match self.changed_cols.get_mut(&col) {
            Some(col_options) => col_options.xf_index = xf_index,
            None => {
                let col_options = ColOptions {
                    width: DEFAULT_COL_WIDTH,
                    xf_index,
                    hidden: false,
                    autofit: false,
                };
                self.changed_cols.insert(col, col_options);
            }
        }

        Ok(self)
    }

    /// Hide a worksheet column.
    ///
    /// The `set_column_hidden()` method is used to hide a column. This can be
    /// used, for example, to hide intermediary steps in a complicated
    /// calculation.
    ///
    /// # Arguments
    ///
    /// * `col` - The zero indexed column number.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Column exceeds Excel's worksheet
    ///   limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates hiding a worksheet column.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_column_hidden.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Hide column B.
    ///     worksheet.set_column_hidden(1)?;
    ///
    ///     worksheet.write_string(0, 3, "Column B is hidden")?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_column_hidden.png">
    ///
    pub fn set_column_hidden(&mut self, col: ColNum) -> Result<&mut Worksheet, XlsxError> {
        // Check if column is in the allowed range without updating dimensions.
        if col >= COL_MAX {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Update an existing col metadata object or create a new one.
        match self.changed_cols.get_mut(&col) {
            Some(col_options) => col_options.hidden = true,
            None => {
                let col_options = ColOptions {
                    width: DEFAULT_COL_WIDTH,
                    xf_index: 0,
                    hidden: true,
                    autofit: false,
                };
                self.changed_cols.insert(col, col_options);
            }
        }

        Ok(self)
    }

    /// Set the autofilter area in the worksheet.
    ///
    /// The `autofilter()` method allows an autofilter to be added to a
    /// worksheet. An autofilter is a way of adding drop down lists to the
    /// headers of a 2D range of worksheet data. This allows users to filter the
    /// data based on simple criteria so that some data is shown and some is
    /// hidden.
    ///
    /// See the [`filter_column`](Worksheet::filter_column) method for an
    /// explanation of how to set a filter conditions for columns in the
    /// autofilter range.
    ///
    /// Note, Excel only allows one autofilter range per worksheet so calling
    /// this method multiple times will overwrite the previous range.
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting a simple autofilter in a
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_autofilter.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet with some sample data to filter.
    /// #     let worksheet = workbook.add_worksheet();
    /// #     worksheet.write_string(0, 0, "Region")?;
    /// #     worksheet.write_string(1, 0, "East")?;
    /// #     worksheet.write_string(2, 0, "West")?;
    /// #     worksheet.write_string(3, 0, "East")?;
    /// #     worksheet.write_string(4, 0, "North")?;
    /// #     worksheet.write_string(5, 0, "South")?;
    /// #     worksheet.write_string(6, 0, "West")?;
    /// #
    /// #     worksheet.write_string(0, 1, "Sales")?;
    /// #     worksheet.write_number(1, 1, 3000)?;
    /// #     worksheet.write_number(2, 1, 8000)?;
    /// #     worksheet.write_number(3, 1, 5000)?;
    /// #     worksheet.write_number(4, 1, 4000)?;
    /// #     worksheet.write_number(5, 1, 7000)?;
    /// #     worksheet.write_number(6, 1, 9000)?;
    /// #
    ///     // Set the autofilter.
    ///     worksheet.autofilter(0, 0, 6, 1)?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_autofilter.png">
    ///
    pub fn autofilter(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows and cols are in the allowed range.
        if !self.check_dimensions_only(first_row, first_col)
            || !self.check_dimensions_only(last_row, last_col)
        {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_row > last_row || first_col > last_col {
            return Err(XlsxError::RowColumnOrderError);
        }

        // Store the defined name information.
        self.autofilter_defined_name.in_use = true;
        self.autofilter_defined_name.name_type = DefinedNameType::Autofilter;
        self.autofilter_defined_name.first_row = first_row;
        self.autofilter_defined_name.first_col = first_col;
        self.autofilter_defined_name.last_row = last_row;
        self.autofilter_defined_name.last_col = last_col;

        self.autofilter_area = utility::cell_range(first_row, first_col, last_row, last_col);

        // Clear any previous filters.
        self.filter_conditions = HashMap::new();

        Ok(self)
    }

    /// Set the filter condition for a column in an autofilter range.
    ///
    /// The [`autofilter()`](Worksheet::autofilter) method sets the cell range
    /// for an autofilter but in order to filter rows within the filter area you
    /// must also add a filter condition.
    ///
    /// Excel supports two main types of filter. The first, and most common, is
    /// a list filter where the user selects the items to filter from a list of
    /// all the values in the the column range:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/autofilter_list.png">
    ///
    /// The other main type of filter is a custom filter where the user can
    /// specify 1 or 2 conditions like ">= 4000" and "<= 6000":
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/autofilter_custom.png">
    ///
    /// In Excel these are mutually exclusive and you will need to choose one or
    /// the other via the [`FilterCondition`] struct parameter.
    ///
    /// For more details on setting filter conditions see [`FilterCondition`]
    /// and the [Working with Autofilters] section of the Users Guide.
    ///
    /// [Working with Autofilters]:
    ///     https://rustxlsxwriter.github.io/formulas/autofilters.html
    ///
    /// Note, there are some limitations on autofilter conditions. The main one
    /// is that the hiding of rows that don't match a filter is not an automatic
    /// part of the file format. Instead it is necessary to hide rows that don't
    /// match the filters. The `rust_xlsxwriter` library does this automatically
    /// and in most cases will get it right, however, there may be cases where
    /// you need to manually hide some of the rows. See [Auto-hiding filtered
    /// rows].
    ///
    /// [Auto-hiding filtered rows]:
    ///     https://rustxlsxwriter.github.io/formulas/autofilters.html#auto-hiding-filtered-rows
    ///
    /// # Arguments
    ///
    /// * `col` - The zero indexed column number.
    /// * `filter_condition` - The column filter condition defined by the
    ///   [`FilterCondition`] struct.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Column exceeds Excel's worksheet
    ///   limits.
    /// * [`XlsxError::ParameterError`] - Parameter error for the following
    ///   issues:
    ///   - The [`autofilter()`](Worksheet::autofilter) range hasn't been set.
    ///   - The column is outside the [`autofilter()`](Worksheet::autofilter)
    ///     range.
    ///   - The [`FilterCondition`] doesn't have a condition set.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting an autofilter with a list
    /// filter condition.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_filter_column1.rs
    /// #
    /// # use rust_xlsxwriter::{FilterCondition, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet with some sample data to filter.
    /// #     let worksheet = workbook.add_worksheet();
    /// #     worksheet.write_string(0, 0, "Region")?;
    /// #     worksheet.write_string(1, 0, "East")?;
    /// #     worksheet.write_string(2, 0, "West")?;
    /// #     worksheet.write_string(3, 0, "East")?;
    /// #     worksheet.write_string(4, 0, "North")?;
    /// #     worksheet.write_string(5, 0, "South")?;
    /// #     worksheet.write_string(6, 0, "West")?;
    /// #
    /// #     worksheet.write_string(0, 1, "Sales")?;
    /// #     worksheet.write_number(1, 1, 3000)?;
    /// #     worksheet.write_number(2, 1, 8000)?;
    /// #     worksheet.write_number(3, 1, 5000)?;
    /// #     worksheet.write_number(4, 1, 4000)?;
    /// #     worksheet.write_number(5, 1, 7000)?;
    /// #     worksheet.write_number(6, 1, 9000)?;
    /// #
    /// #     // Set the autofilter.
    /// #     worksheet.autofilter(0, 0, 6, 1)?;
    /// #
    ///     // Set a filter condition to only show cells matching "East" in the first
    ///     // column.
    ///     let filter_condition = FilterCondition::new().add_list_filter("East");
    ///     worksheet.filter_column(0, &filter_condition)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_filter_column1.png">
    ///
    pub fn filter_column(
        &mut self,
        col: ColNum,
        filter_condition: &FilterCondition,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check if column is in the allowed range without updating dimensions.
        if col >= COL_MAX {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check that an autofilter has been created before a condition can be
        // applied to it.
        if !self.autofilter_defined_name.in_use {
            let error =
                "The 'autofilter()' range must be set before a 'filter_condition' can be applied."
                    .to_string();
            return Err(XlsxError::ParameterError(error));
        }

        // Check if column is within the autofilter column range.
        if col < self.autofilter_defined_name.first_col
            || col > self.autofilter_defined_name.last_col
        {
            let error = format!(
                "Col '{col}' outside user defined autofilter column range '{}-{}'",
                self.autofilter_defined_name.first_col, self.autofilter_defined_name.last_col
            );
            return Err(XlsxError::ParameterError(error));
        }

        // Check the filter condition have been set up correctly.
        if filter_condition.list.is_empty()
            && filter_condition.custom1.is_none()
            && !filter_condition.should_match_blanks
        {
            let error =
                "The 'filter_condition' doesn't have a data value or condition set.".to_string();
            return Err(XlsxError::ParameterError(error));
        }

        self.filter_conditions.insert(col, filter_condition.clone());

        Ok(self)
    }

    /// Turn off the option to automatically hide rows that don't match filters.
    ///
    /// Rows that don't match autofilter conditions are hidden by Excel at
    /// runtime. This feature isn't an automatic part of the file format and in
    /// practice it is necessary for the user to hide rows that don't match the
    /// applied filters. The `rust_xlsxwriter` library tries to do this
    /// automatically and in most cases will get it right, however, there may be
    /// cases where you need to manually hide some of the rows and may want to
    /// turn off the automatic handling using `filter_automatic_off()`.
    ///
    /// See [Auto-hiding filtered rows] in the User Guide.
    ///
    /// [Auto-hiding filtered rows]:
    ///     https://rustxlsxwriter.github.io/formulas/autofilters.html#auto-hiding-filtered-rows
    ///
    pub fn filter_automatic_off(&mut self) -> &mut Worksheet {
        self.filter_automatic_off = true;
        self
    }

    /// Protect a worksheet from modification.
    ///
    /// The `protect()` method protects a worksheet from modification. It works
    /// by enabling a cell's `locked` and `hidden` properties, if they have been
    /// set. A **locked** cell cannot be edited and this property is on by
    /// default for all cells. A **hidden** cell will display the results of a
    /// formula but not the formula itself.
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/protection_alert.png">
    ///
    /// These properties can be set using the
    /// [`format.set_locked()`](Format::set_locked)
    /// [`format.set_unlocked()`](Format::set_unlocked) and
    /// [`worksheet.set_hidden()`](Format::set_hidden) format methods. All cells
    /// have the `locked` property turned on by default (see the example below)
    /// so in general you don't have to explicitly turn it on.
    ///
    /// # Examples
    ///
    /// Example of cell locking and formula hiding in an Excel worksheet
    /// rust_xlsxwriter library.
    ///
    /// ```
    /// # // This code is available in examples/app_worksheet_protection.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Create some format objects.
    ///     let unlocked = Format::new().set_unlocked();
    ///     let hidden = Format::new().set_hidden();
    ///
    ///     // Protect the worksheet to turn on cell locking.
    ///     worksheet.protect();
    ///
    ///     // Examples of cell locking and hiding.
    ///     worksheet.write_string(0, 0, "Cell B1 is locked. It cannot be edited.")?;
    ///     worksheet.write_formula(0, 1, "=1+2")?; // Locked by default.
    ///
    ///     worksheet.write_string(1, 0, "Cell B2 is unlocked. It can be edited.")?;
    ///     worksheet.write_formula_with_format(1, 1, "=1+2", &unlocked)?;
    ///
    ///     worksheet.write_string(2, 0, "Cell B3 is hidden. The formula isn't visible.")?;
    ///     worksheet.write_formula_with_format(2, 1, "=1+2", &hidden)?;
    ///
    /// #     worksheet.write_string(4, 0, "Use Menu -> Review -> Unprotect Sheet")?;
    /// #     worksheet.write_string(5, 0, "to remove the worksheet protection.")?;
    /// #
    /// #     worksheet.autofit();
    /// #
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet_protection.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/app_worksheet_protection.png">
    ///
    pub fn protect(&mut self) -> &mut Worksheet {
        self.protection_on = true;

        self
    }

    /// Protect a worksheet from modification with a password.
    ///
    /// The `protect_with_password()` method is like the
    /// [`protect()`](Worksheet::protect) method, see above, except that you can
    /// add an optional, weak, password to prevent modification.
    ///
    /// **Note**: Worksheet level passwords in Excel offer very weak protection.
    /// They do not encrypt your data and are very easy to deactivate. Full
    /// workbook encryption is not supported by `rust_xlsxwriter`. However, it
    /// is possible to encrypt an rust_xlsxwriter file using a third party open
    /// source tool called [msoffice-crypt](https://github.com/herumi/msoffice).
    /// This works for macOS, Linux and Windows:
    ///
    /// ```text
    /// msoffice-crypt.exe -e -p password clear.xlsx encrypted.xlsx
    /// ```
    ///
    /// # Arguments
    ///
    /// * `password` - The password string. Note, only ascii text passwords are
    ///   supported. Passing the empty string "" is the same as turning on
    ///   protection without a password.
    ///
    /// # Examples
    ///
    /// The following example demonstrates protecting a worksheet from editing
    /// with a password.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_protect_with_password.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     // Add a worksheet to the workbook.
    ///     let worksheet = workbook.add_worksheet();
    ///
    ///     // Protect the worksheet from modification.
    ///     worksheet.protect_with_password("abc123");
    ///
    /// #     worksheet.write_string(0, 0, "Unlock the worksheet to edit the cell")?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_protect_with_password.png">
    ///
    pub fn protect_with_password(&mut self, password: &str) -> &mut Worksheet {
        self.protection_on = true;
        self.protection_hash = utility::hash_password(password);

        self
    }

    /// Specify which worksheet elements should, or shouldn't, be protected.
    ///
    /// The `protect_with_password()` method is like the
    /// [`protect()`](Worksheet::protect) method, see above, except it also
    /// specifies which worksheet elements should, or shouldn't, be protected.
    ///
    /// You can specify which worksheet elements protection should be on or off
    /// via a [`ProtectionOptions`] struct reference. The Excel options
    /// with their default states are shown below:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_protect_with_options1.png">
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet properties to
    /// be protected in a protected worksheet. In this case we protect the
    /// overall worksheet but allow columns and rows to be inserted.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_protect_with_options.rs
    /// #
    /// # use rust_xlsxwriter::{ProtectionOptions, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Set some of the options and use the defaults for everything else.
    ///     let options = ProtectionOptions {
    ///         insert_columns: true,
    ///         insert_rows: true,
    ///         ..ProtectionOptions::default()
    ///     };
    ///
    ///     // Set the protection options.
    ///     worksheet.protect_with_options(&options);
    ///
    /// #     worksheet.write_string(0, 0, "Unlock the worksheet to edit the cell")?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Excel dialog for the output file, compare this with the default image
    /// above:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_protect_with_options2.png">
    ///
    pub fn protect_with_options(&mut self, options: &ProtectionOptions) -> &mut Worksheet {
        self.protection_on = true;
        self.protection_options = options.clone();

        self
    }

    /// Unprotect a range of cells in a protected worksheet.
    ///
    /// As shown in the example for the
    /// [`worksheet.protect()`](Worksheet::protect) method it is possible to
    /// unprotect a cell by setting the format `unprotect` property. Excel also
    /// offers an interface to unprotect larger ranges of cells. This is
    /// replicated in `rust_xlsxwriter` using the `unprotect_range()` method,
    /// see the example below.
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    ///
    /// # Examples
    ///
    /// The following example demonstrates unprotecting ranges in a protected
    /// worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_unprotect_range.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Protect the worksheet from modification.
    ///     worksheet.protect();
    ///
    ///     // Unprotect range D4:F10.
    ///     worksheet.unprotect_range(4, 3, 9, 5)?;
    ///
    ///     // Unprotect single cell B3 by repeating (row, col).
    ///     worksheet.unprotect_range(2, 1, 2, 1)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Dialog from the output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_unprotect_range.png">
    ///
    pub fn unprotect_range(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        self.unprotect_range_with_options(first_row, first_col, last_row, last_col, "", "")
    }

    /// Unprotect a range of cells in a protected worksheet, with options.
    ///
    /// This method is similar to
    /// `unprotect_range()`[Worksheet::unprotect_range], see above, expect that
    /// it allows you to specify two additional parameters to set the name of
    /// the range (instead of the default Range1 .. RangeN) and also a optional
    /// weak password (see
    /// [`protect_with_password()`](Worksheet::protect_with_password) for an
    /// explanation of what weak means here).
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    /// * `name` - The name of the range instead of RangeN. Can be blank if not
    ///   required.
    /// * `password` - The password to prevent modification of the range. Can be
    ///   blank if not required.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    ///
    /// # Examples
    ///
    /// The following example demonstrates unprotecting ranges in a protected
    /// worksheet, with additional options.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_unprotect_range_with_options.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Protect the worksheet from modification.
    ///     worksheet.protect();
    ///
    ///     // Unprotect range D4:F10 and give it a user defined name.
    ///     worksheet.unprotect_range_with_options(4, 3, 9, 5, "MyRange", "")?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Dialog from the output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_unprotect_range_with_options.png">
    ///
    pub fn unprotect_range_with_options(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        name: &str,
        password: &str,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows and cols are in the allowed range.
        if !self.check_dimensions_only(first_row, first_col)
            || !self.check_dimensions_only(last_row, last_col)
        {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_row > last_row || first_col > last_col {
            return Err(XlsxError::RowColumnOrderError);
        }

        let range = utility::cell_range(first_row, first_col, last_row, last_col);
        let mut name = name.to_string();
        let password_hash = utility::hash_password(password);

        if name.is_empty() {
            name = format!("Range{}", 1 + self.unprotected_ranges.len());
        }

        self.unprotected_ranges.push((range, name, password_hash));

        Ok(self)
    }

    /// Set the selected cell or cells in a worksheet.
    ///
    /// The `set_selection()` method can be used to specify which cell or range
    /// of cells is selected in a worksheet. The most common requirement is to
    /// select a single cell, in which case the `first_` and `last_` parameters
    /// should be the same.
    ///
    /// The active cell within a selected range is determined by the order in
    /// which `first_` and `last_` are specified.
    ///
    /// Only one range of cells can be selected. The default cell selection is
    /// (0, 0, 0, 0), "A1".
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates selecting cells in worksheets. The order
    /// of selection within the range depends on the order of `first` and `last`.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_selection.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = workbook.add_worksheet();
    ///     worksheet1.set_selection(3, 2, 3, 2)?; // Cell C4
    ///
    ///     let worksheet2 = workbook.add_worksheet();
    ///     worksheet2.set_selection(3, 2, 6, 6)?; // Cells C4 to G7.
    ///
    ///     let worksheet3 = workbook.add_worksheet();
    ///     worksheet3.set_selection(6, 6, 3, 2)?; // Cells G7 to C4.
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_selection.png">
    pub fn set_selection(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows and cols are in the allowed range.
        if !self.check_dimensions_only(first_row, first_col)
            || !self.check_dimensions_only(last_row, last_col)
        {
            return Err(XlsxError::RowColumnLimitError);
        }

        // The first/last order can be reversed to allow a selection to go from
        // the end to the start. We take the active cell from the user first
        // row/col and then reverse them as required for the full range.
        let active_cell = utility::rowcol_to_cell(first_row, first_col);

        let mut first_row = first_row;
        let mut first_col = first_col;
        let mut last_row = last_row;
        let mut last_col = last_col;

        if first_row > last_row {
            std::mem::swap(&mut first_row, &mut last_row);
        }

        if first_col > last_col {
            std::mem::swap(&mut first_col, &mut last_col);
        }

        let range = utility::cell_range(first_row, first_col, last_row, last_col);

        self.selected_range = (active_cell, range);

        Ok(self)
    }

    /// Set the first visible cell at the top left of a worksheet.
    ///
    /// This `set_top_left_cell()` method can be used to set the top leftmost
    /// visible cell in the worksheet.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the top and leftmost visible
    /// cell in the worksheet. Often used in conjunction with `set_selection()`
    /// to activate the same cell.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_top_left_cell.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #    let worksheet = workbook.add_worksheet();
    /// #
    ///     // Set top-left cell to AA32.
    ///     worksheet.set_top_left_cell(31, 26)?;
    ///
    ///     // Also make this the active/selected cell.
    ///     worksheet.set_selection(31, 26, 31, 26)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_top_left_cell.png">
    ///
    pub fn set_top_left_cell(
        &mut self,
        row: RowNum,
        col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and col are in the allowed range.
        if !self.check_dimensions_only(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Ignore cell (0, 0) since that is the default top-left cell.
        if row == 0 && col == 0 {
            return Ok(self);
        }

        self.top_left_cell = utility::rowcol_to_cell(row, col);

        Ok(self)
    }

    /// Write a user defined result to a worksheet formula cell.
    ///
    /// The `rust_xlsxwriter` library doesn’t calculate the result of a formula
    /// written using [`write_formula_with_format()`](Worksheet::write_formula_with_format()) or
    /// [`write_formula()`](Worksheet::write_formula()). Instead it
    /// stores the value 0 as the formula result. It then sets a global flag in
    /// the xlsx file to say that all formulas and functions should be
    /// recalculated when the file is opened.
    ///
    /// This works fine with Excel and other spreadsheet applications. However,
    /// applications that don’t have a facility to calculate formulas will only
    /// display the 0 results.
    ///
    /// If required, it is possible to specify the calculated result of a
    /// formula using the `set_formula_result()` method.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    /// * `result` - The formula result to write to the cell.
    ///
    /// # Warnings
    ///
    /// You will get a warning if you try to set a formula result for a cell
    /// that doesn't have a formula.
    ///
    /// # Examples
    ///
    /// The following example demonstrates manually setting the result of a
    /// formula.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_formula_result.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet
    ///         .write_formula(0, 0, "1+1")?
    ///         .set_formula_result(0, 0, "2");
    ///
    /// #     workbook.save("formulas.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    pub fn set_formula_result(&mut self, row: RowNum, col: ColNum, result: &str) -> &mut Worksheet {
        if let Some(columns) = self.table.get_mut(&row) {
            if let Some(cell) = columns.get_mut(&col) {
                match cell {
                    CellType::Formula {
                        formula: _,
                        xf_index: _,
                        result: cell_result,
                    } => {
                        *cell_result = result.to_string();
                    }
                    CellType::ArrayFormula {
                        formula: _,
                        xf_index: _,
                        result: cell_result,
                        is_dynamic: _,
                        range: _,
                    } => {
                        *cell_result = result.to_string();
                    }
                    _ => {
                        eprintln!("Cell ({row}, {col}) doesn't contain a formula.");
                    }
                }
            }
        }

        self
    }

    /// Write the default formula result for worksheet formulas.
    ///
    /// The `rust_xlsxwriter` library doesn’t calculate the result of a formula
    /// written using [`write_formula_with_format()`](Worksheet::write_formula_with_format()) or
    /// [`write_formula()`](Worksheet::write_formula()). Instead it
    /// stores the value 0 as the formula result. It then sets a global flag in
    /// the xlsx file to say that all formulas and functions should be
    /// recalculated when the file is opened.
    ///
    /// However, for LibreOffice the default formula result should be set to the
    /// empty string literal `""`, via the `set_formula_result_default()`
    /// method, to force calculation of the result.
    ///
    /// # Arguments
    ///
    /// * `result` - The default formula result to write to the cell.
    ///
    /// # Examples
    ///
    /// The following example demonstrates manually setting the default result
    /// for all non-calculated formulas in a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_formula_result_default.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet.set_formula_result_default("");
    ///
    /// #     workbook.save("formulas.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// #
    pub fn set_formula_result_default(&mut self, result: &str) -> &mut Worksheet {
        self.default_result = result.to_string();
        self
    }

    /// Enable the use of newer Excel future functions.
    ///
    /// Enable the use of newer Excel “future” functions without having to
    /// prefix them with with `_xlfn`.
    ///
    /// Excel 2010 and later versions added functions which weren't defined in
    /// the original file specification. These functions are referred to by
    /// Microsoft as "Future Functions". Examples of these functions are `ACOT`,
    /// `CHISQ.DIST.RT` , `CONFIDENCE.NORM`, `STDEV.P`, `STDEV.S` and
    /// `WORKDAY.INTL`.
    ///
    /// When written using [`write_formula_with_format()`](Worksheet::write_formula_with_format()) these
    /// functions need to be fully qualified with a prefix such as `_xlfn.`
    ///
    /// Alternatively you can use the `worksheet.use_future_functions()`
    /// function to have `rust_xlsxwriter` automatically handle future functions
    /// for you, see below.
    ///
    /// # Examples
    ///
    /// The following example demonstrates writing an Excel "Future Function"
    /// with an implicit prefix and the use_future_functions() method.
    ///
    /// ```
    /// # // This code is available in examples/doc_working_with_formulas_future3.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     // Write a future function and automatically add the required prefix.
    ///     worksheet.use_future_functions(true);
    ///     worksheet.write_formula(0, 0, "=STDEV.S(B1:B5)")?;
    /// #
    /// #     // Write some data for the function to operate on.
    /// #     worksheet.write_number(0, 1, 1.23)?;
    /// #     worksheet.write_number(1, 1, 1.03)?;
    /// #     worksheet.write_number(2, 1, 1.20)?;
    /// #     worksheet.write_number(3, 1, 1.15)?;
    /// #     worksheet.write_number(4, 1, 1.22)?;
    /// #
    /// #     workbook.save("future_function.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/working_with_formulas2.png">
    ///
    pub fn use_future_functions(&mut self, enable: bool) {
        self.use_future_functions = enable;
    }

    // -----------------------------------------------------------------------
    // Worksheet page setup methods.
    // -----------------------------------------------------------------------

    /// Display the worksheet cells from right to left for some versions of
    /// Excel.
    ///
    /// The `set_right_to_left()` method is used to change the default direction
    /// of the worksheet from left-to-right, with the A1 cell in the top left,
    /// to right-to-left, with the A1 cell in the top right.
    ///
    /// This is useful when creating Arabic, Hebrew or other near or far eastern
    /// worksheets that use right-to-left as the default direction.
    ///
    /// Depending on your use case, and text, you may also need to use the
    /// [`Format::set_reading_direction()`](crate::Format::set_reading_direction)
    /// method to set the direction of the text within the cells.
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    /// # Examples
    ///
    /// The following example demonstrates changing the default worksheet and
    /// cell text direction changed from left-to-right to right-to-left, as
    /// required by some middle eastern versions of Excel.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_right_to_left.rs
    /// #
    /// # use rust_xlsxwriter::{Format, Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     // Add the cell formats.
    ///     let format_left_to_right = Format::new().set_reading_direction(1);
    ///     let format_right_to_left = Format::new().set_reading_direction(2);
    ///
    ///     // Add a worksheet in the standard left to right direction.
    ///     let worksheet1 = workbook.add_worksheet();
    ///
    ///     // Make the column wider for clarity.
    ///     worksheet1.set_column_width(0,25)?;
    ///
    ///     // Standard direction:         | A1 | B1 | C1 | ...
    ///     worksheet1.write_string(0, 0, "نص عربي / English text")?;
    ///     worksheet1.write_string_with_format(1, 0, "نص عربي / English text", &format_left_to_right)?;
    ///     worksheet1.write_string_with_format(2, 0, "نص عربي / English text", &format_right_to_left)?;
    ///
    ///     // Add a worksheet and change it to right to left direction.
    ///     let worksheet2 = workbook.add_worksheet();
    ///     worksheet2.set_right_to_left(true);
    ///
    ///     // Make the column wider for clarity.
    ///     worksheet2.set_column_width(0, 25)?;
    ///
    ///     // Right to left direction:    ... | C1 | B1 | A1 |
    ///     worksheet2.write_string(0, 0, "نص عربي / English text")?;
    ///     worksheet2.write_string_with_format(1, 0, "نص عربي / English text", &format_left_to_right)?;
    ///     worksheet2.write_string_with_format(2, 0, "نص عربي / English text", &format_right_to_left)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_right_to_left.png">
    ///
    pub fn set_right_to_left(&mut self, enable: bool) -> &mut Worksheet {
        self.right_to_left = enable;
        self
    }

    /// Make a worksheet the active/initially visible worksheet in a workbook.
    ///
    /// The `set_active()` method is used to specify which worksheet is
    /// initially visible in a multi-sheet workbook. If no worksheet is set then
    /// the first worksheet is made the active worksheet, like in Excel.
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting a worksheet as the visible
    /// worksheet when a file is opened.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_active.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = Worksheet::new();
    ///     let worksheet3 = Worksheet::new();
    ///     let mut worksheet2 = Worksheet::new();
    ///
    ///     worksheet2.set_active(true);
    ///
    /// #   workbook.push_worksheet(worksheet1);
    /// #   workbook.push_worksheet(worksheet2);
    /// #   workbook.push_worksheet(worksheet3);
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_active.png">
    ///
    pub fn set_active(&mut self, enable: bool) -> &mut Worksheet {
        self.active = enable;

        // Activated worksheets must also be selected and cannot be hidden.
        if self.active {
            self.selected = true;
            self.hidden = false;
        }

        self
    }

    /// Set a worksheet tab as selected.
    ///
    /// The `set_selected()` method is used to indicate that a worksheet is
    /// selected in a multi-sheet workbook.
    ///
    /// A selected worksheet has its tab highlighted. Selecting worksheets is a
    /// way of grouping them together so that, for example, several worksheets
    /// could be printed in one go. A worksheet that has been activated via the
    /// [`set_active()`](Worksheet::set_active) method will also appear as
    /// selected.
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    /// # Examples
    ///
    /// The following example demonstrates selecting worksheet in a workbook. The
    /// active worksheet is selected by default so in this example the first two
    /// worksheets are selected.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_selected.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = Worksheet::new();
    ///     let worksheet3 = Worksheet::new();
    ///     let mut worksheet2 = Worksheet::new();
    ///
    ///     worksheet2.set_selected(true);
    ///
    /// #   workbook.push_worksheet(worksheet1);
    /// #   workbook.push_worksheet(worksheet2);
    /// #   workbook.push_worksheet(worksheet3);
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_selected.png">
    ///
    pub fn set_selected(&mut self, enable: bool) -> &mut Worksheet {
        self.selected = enable;

        // Selected worksheets cannot be hidden.
        if self.selected {
            self.hidden = false;
        }

        self
    }

    /// Hide a worksheet.
    ///
    /// The `set_hidden()` method is used to hide a worksheet. This can be used
    /// to hide a worksheet in order to avoid confusing a user with intermediate
    /// data or calculations.
    ///
    /// In Excel a hidden worksheet can not be activated or selected so this
    /// method is mutually exclusive with the
    /// [`set_active()`](Worksheet::set_active) and
    /// [`set_selected()`](Worksheet::set_selected) methods. In addition, since
    /// the first worksheet will default to being the active worksheet, you
    /// cannot hide the first worksheet without activating another sheet.
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    /// # Examples
    ///
    /// The following example demonstrates hiding a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_hidden.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = Worksheet::new();
    ///     let worksheet3 = Worksheet::new();
    ///     let mut worksheet2 = Worksheet::new();
    ///
    ///     worksheet2.set_hidden(true);
    ///
    /// #    workbook.push_worksheet(worksheet1);
    /// #    workbook.push_worksheet(worksheet2);
    /// #    workbook.push_worksheet(worksheet3);
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_hidden.png">
    ///
    pub fn set_hidden(&mut self, enable: bool) -> &mut Worksheet {
        self.hidden = enable;

        // Hidden worksheets cannot be active or hidden.
        if self.hidden {
            self.selected = false;
            self.active = false;
        }

        self
    }

    /// Set current worksheet as the first visible sheet tab.
    ///
    /// The [`set_active()`](Worksheet::set_active)  method determines
    /// which worksheet is initially selected. However, if there are a large
    /// number of worksheets the selected worksheet may not appear on the
    /// screen. To avoid this you can select which is the leftmost visible
    /// worksheet tab using `set_first_tab()`.
    ///
    /// This method is not required very often. The default is the first
    /// worksheet.
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_first_tab(&mut self, enable: bool) -> &mut Worksheet {
        self.first_sheet = enable;

        // First visible worksheet cannot be hidden.
        if self.selected {
            self.hidden = false;
        }
        self
    }

    /// Set the color of the worksheet tab.
    ///
    /// The `set_tab_color()` method can be used to change the color of the
    /// worksheet tab. This is useful for highlighting the important tab in a
    /// group of worksheets.
    ///
    /// # Arguments
    ///
    /// * `color` - The tab color property defined by a [`XlsxColor`] enum
    ///   value.
    ///
    /// # Examples
    ///
    /// The following example demonstrates set the tab color of worksheets.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_tab_color.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, Worksheet, XlsxColor, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let mut worksheet1 = Worksheet::new();
    ///     let mut worksheet2 = Worksheet::new();
    ///     let mut worksheet3 = Worksheet::new();
    ///     let mut worksheet4 = Worksheet::new();
    ///
    ///     worksheet1.set_tab_color(XlsxColor::Red);
    ///     worksheet2.set_tab_color(XlsxColor::Green);
    ///     worksheet3.set_tab_color(XlsxColor::RGB(0xFF9900));
    ///
    ///     // worksheet4 will have the default color.
    ///     worksheet4.set_active(true);
    ///
    /// #    workbook.push_worksheet(worksheet1);
    /// #    workbook.push_worksheet(worksheet2);
    /// #    workbook.push_worksheet(worksheet3);
    /// #    workbook.push_worksheet(worksheet4);
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_tab_color.png">
    ///
    pub fn set_tab_color(&mut self, color: XlsxColor) -> &mut Worksheet {
        if !color.is_valid() {
            return self;
        }

        self.tab_color = color;
        self
    }

    /// Set the paper type/size when printing.
    ///
    /// This method is used to set the paper format for the printed output of a
    /// worksheet. The following paper styles are available:
    ///
    /// | Index    | Paper format            | Paper size           |
    /// | :------- | :---------------------- | :------------------- |
    /// | 0        | Printer default         | Printer default      |
    /// | 1        | Letter                  | 8 1/2 x 11 in        |
    /// | 2        | Letter Small            | 8 1/2 x 11 in        |
    /// | 3        | Tabloid                 | 11 x 17 in           |
    /// | 4        | Ledger                  | 17 x 11 in           |
    /// | 5        | Legal                   | 8 1/2 x 14 in        |
    /// | 6        | Statement               | 5 1/2 x 8 1/2 in     |
    /// | 7        | Executive               | 7 1/4 x 10 1/2 in    |
    /// | 8        | A3                      | 297 x 420 mm         |
    /// | 9        | A4                      | 210 x 297 mm         |
    /// | 10       | A4 Small                | 210 x 297 mm         |
    /// | 11       | A5                      | 148 x 210 mm         |
    /// | 12       | B4                      | 250 x 354 mm         |
    /// | 13       | B5                      | 182 x 257 mm         |
    /// | 14       | Folio                   | 8 1/2 x 13 in        |
    /// | 15       | Quarto                  | 215 x 275 mm         |
    /// | 16       | ---                     | 10x14 in             |
    /// | 17       | ---                     | 11x17 in             |
    /// | 18       | Note                    | 8 1/2 x 11 in        |
    /// | 19       | Envelope 9              | 3 7/8 x 8 7/8        |
    /// | 20       | Envelope 10             | 4 1/8 x 9 1/2        |
    /// | 21       | Envelope 11             | 4 1/2 x 10 3/8       |
    /// | 22       | Envelope 12             | 4 3/4 x 11           |
    /// | 23       | Envelope 14             | 5 x 11 1/2           |
    /// | 24       | C size sheet            | ---                  |
    /// | 25       | D size sheet            | ---                  |
    /// | 26       | E size sheet            | ---                  |
    /// | 27       | Envelope DL             | 110 x 220 mm         |
    /// | 28       | Envelope C3             | 324 x 458 mm         |
    /// | 29       | Envelope C4             | 229 x 324 mm         |
    /// | 30       | Envelope C5             | 162 x 229 mm         |
    /// | 31       | Envelope C6             | 114 x 162 mm         |
    /// | 32       | Envelope C65            | 114 x 229 mm         |
    /// | 33       | Envelope B4             | 250 x 353 mm         |
    /// | 34       | Envelope B5             | 176 x 250 mm         |
    /// | 35       | Envelope B6             | 176 x 125 mm         |
    /// | 36       | Envelope                | 110 x 230 mm         |
    /// | 37       | Monarch                 | 3.875 x 7.5 in       |
    /// | 38       | Envelope                | 3 5/8 x 6 1/2 in     |
    /// | 39       | Fanfold                 | 14 7/8 x 11 in       |
    /// | 40       | German Std Fanfold      | 8 1/2 x 12 in        |
    /// | 41       | German Legal Fanfold    | 8 1/2 x 13 in        |
    ///
    /// Note, it is likely that not all of these paper types will be available
    /// to the end user since it will depend on the paper formats that the
    /// user's printer supports. Therefore, it is best to stick to standard
    /// paper types of 1 for US Letter and 9 for A4.
    ///
    /// If you do not specify a paper type the worksheet will print using the
    /// printer's default paper style.
    ///
    /// # Arguments
    ///
    /// * `paper_size` - The paper size index from the list above .
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet paper size/type for
    /// the printed output.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_paper.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Set the printer paper size.
    ///     worksheet.set_paper_size(9); // A4 paper size.
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    pub fn set_paper_size(&mut self, paper_size: u8) -> &mut Worksheet {
        self.paper_size = paper_size;
        self.page_setup_changed = true;
        self
    }

    /// Set the order in which pages are printed.
    ///
    /// The `set_page_order()` method is used to change the default print
    /// direction. This is referred to by Excel as the sheet "page order":
    ///
    /// The default page order is shown below for a worksheet that extends over
    /// 4 pages. The order is called "down then over":
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_page_order.png">
    ///
    /// However, by using `set_page_order(false)` the print order will be
    /// changed to "over then down".
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. Set `true` to get "Down, then
    ///   over" (the default) and `false` to get "Over, then down".
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet printed page
    /// order.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_page_order.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Set the page print to "over then down"
    ///     worksheet.set_page_order(false);
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    pub fn set_page_order(&mut self, enable: bool) -> &mut Worksheet {
        self.default_page_order = enable;

        if !enable {
            self.page_setup_changed = true;
        }
        self
    }

    /// Set the page orientation to landscape.
    ///
    /// The `set_landscape()` method is used to set the orientation of a
    /// worksheet's printed page to landscape.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet page orientation to
    /// landscape.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_landscape.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet.set_landscape();
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    pub fn set_landscape(&mut self) -> &mut Worksheet {
        self.portrait = false;
        self.page_setup_changed = true;
        self
    }

    /// Set the page orientation to portrait.
    ///
    ///  This `set_portrait()` method  is used to set the orientation of a
    ///  worksheet's printed page to portrait. The default worksheet orientation
    ///  is portrait, so this function is rarely required.
    ///
    pub fn set_portrait(&mut self) -> &mut Worksheet {
        self.portrait = true;
        self.page_setup_changed = true;
        self
    }

    /// Set the page view mode to normal layout.
    ///
    /// This method is used to display the worksheet in “View -> Normal”
    /// mode. This is the default.
    ///
    pub fn set_view_normal(&mut self) -> &mut Worksheet {
        self.page_view = PageView::Normal;
        self
    }

    /// Set the page view mode to page layout.
    ///
    /// This method is used to display the worksheet in “View -> Page Layout”
    /// mode.
    ///
    pub fn set_view_page_layout(&mut self) -> &mut Worksheet {
        self.page_view = PageView::PageLayout;
        self.page_setup_changed = true;
        self
    }

    /// Set the page view mode to page break preview.
    ///
    /// This method is used to display the worksheet in “View -> Page Break
    /// Preview” mode.
    ///
    pub fn set_view_page_break_preview(&mut self) -> &mut Worksheet {
        self.page_view = PageView::PageBreaks;
        self.page_setup_changed = true;
        self
    }

    /// Set the horizontal page breaks on a worksheet.
    ///
    /// The `set_page_breaks()` method adds horizontal page breaks to a
    /// worksheet. A page break causes all the data that follows it to be
    /// printed on the next page. Horizontal page breaks act between rows.
    ///
    /// # Arguments
    ///
    /// * `breaks` - A list of one or more row numbers where the page breaks
    ///   occur. To create a page break between rows 20 and 21 you must specify
    ///   the break at row 21. However in zero index notation this is actually
    ///   row 20. So you can pretend for a small while that you are using 1
    ///   index notation.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::ParameterError`] - The number of page breaks exceeds
    ///   Excel's limit of 1023 page breaks.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting page breaks for a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_page_breaks.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     let worksheet = workbook.add_worksheet();
    /// #     worksheet.write_string(100, 100, "Test")?;
    /// #
    ///     // Set a page break at rows 20, 40 and 60.
    ///     worksheet.set_page_breaks(&[20, 40, 60])?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_page_breaks.png">
    ///
    pub fn set_page_breaks(&mut self, breaks: &[RowNum]) -> Result<&mut Worksheet, XlsxError> {
        // Ignore empty input.
        if breaks.is_empty() {
            return Ok(self);
        }

        // Sort list and remove any duplicates and 0.
        let breaks = self.process_pagebreaks(breaks)?;

        // Check max break value is within Excel column limit.
        if *breaks.last().unwrap() >= ROW_MAX {
            return Err(XlsxError::RowColumnLimitError);
        }

        self.horizontal_breaks = breaks;

        Ok(self)
    }

    /// Set the vertical page breaks on a worksheet.
    ///
    /// The `set_vertical_page_breaks()` method adds vertical page breaks to a
    /// worksheet. This is much less common than the
    /// [`set_page_breaks()`](Worksheet::set_page_breaks) method shown above.
    ///
    /// # Arguments
    ///
    /// * `breaks` - A list of one or more column numbers where the page breaks
    ///   occur.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::ParameterError`] - The number of page breaks exceeds
    ///   Excel's limit of 1023 page breaks.
    ///
    pub fn set_vertical_page_breaks(
        &mut self,
        breaks: &[u32],
    ) -> Result<&mut Worksheet, XlsxError> {
        // Ignore empty input.
        if breaks.is_empty() {
            return Ok(self);
        }

        // Sort list and remove any duplicates and 0.
        let breaks = self.process_pagebreaks(breaks)?;

        // Check max break value is within Excel col limit.
        if *breaks.last().unwrap() >= COL_MAX as u32 {
            return Err(XlsxError::RowColumnLimitError);
        }

        self.vertical_breaks = breaks;

        Ok(self)
    }

    /// Set the worksheet zoom factor.
    ///
    /// Set the worksheet zoom factor in the range 10 <= zoom <= 400.
    ///
    /// The default zoom level is 100. The `set_zoom()` method does not affect
    /// the scale of the printed page in Excel. For that you should use
    /// [`set_print_scale()`](Worksheet::set_print_scale).
    ///
    /// # Arguments
    ///
    /// * `zoom` - The worksheet zoom level.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet zoom level.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_zoom.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet.write_string(0, 0, "Hello")?;
    ///     worksheet.set_zoom(200);
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_zoom.png">
    ///
    pub fn set_zoom(&mut self, zoom: u16) -> &mut Worksheet {
        if !(10..=400).contains(&zoom) {
            eprintln!("Zoom factor {zoom} outside Excel range: 10 <= zoom <= 400.");
            return self;
        }

        self.zoom = zoom;
        self
    }

    /// Freeze panes in a worksheet.
    ///
    /// The `set_freeze_panes()` method can be used to divide a worksheet into
    /// horizontal or vertical regions known as panes and to “freeze” these
    /// panes so that the splitter bars are not visible.
    ///
    /// As with Excel the split is to the top and left of the cell. So to freeze
    /// the top row and leftmost column you would use `(1, 1)` (zero-indexed).
    /// Also, you can set one of the row and col parameters as 0 if you do not
    /// want either the vertical or horizontal split. See the example below.
    ///
    /// In Excel it is also possible to set "split" panes without freezing them.
    /// That feature isn't currently supported by rust_xlsxwriter.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet panes.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_freeze_panes.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     let mut worksheet1 = Worksheet::new();
    /// #     let mut worksheet2 = Worksheet::new();
    /// #     let mut worksheet3 = Worksheet::new();
    /// #
    /// #     worksheet1.write_string(0, 0, "Scroll down")?;
    /// #     worksheet2.write_string(0, 0, "Scroll across")?;
    /// #     worksheet3.write_string(0, 0, "Scroll down or across")?;
    /// #
    ///     // Freeze the top row only.
    ///     worksheet1.set_freeze_panes(1, 0)?;
    ///
    ///     // Freeze the leftmost column only.
    ///     worksheet2.set_freeze_panes(0, 1)?;
    ///
    ///     // Freeze the top row and leftmost column.
    ///     worksheet3.set_freeze_panes(1, 1)?;
    ///
    /// #     workbook.push_worksheet(worksheet1);
    /// #     workbook.push_worksheet(worksheet2);
    /// #     workbook.push_worksheet(worksheet3);
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_freeze_panes.png">
    ///
    pub fn set_freeze_panes(
        &mut self,
        row: RowNum,
        col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and col are in the allowed range.
        if !self.check_dimensions_only(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        self.panes.freeze_cell = (row, col);
        Ok(self)
    }

    /// Set the top most cell in the scrolling area of a freeze pane.
    ///
    /// This method is used in conjunction with the
    /// [`set_freeze_panes()`](Worksheet::set_freeze_panes) method to set the
    /// top most visible cell in the scrolling range. For example you may want
    /// to freeze the top row a but have the worksheet pre-scrolled so that cell
    /// `A20` is visible in the scrolled area. See the example below.
    ///
    /// # Arguments
    ///
    /// * `row` - The zero indexed row number.
    /// * `col` - The zero indexed column number.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet panes and also
    /// setting the topmost visible cell in the scrolled area.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_freeze_panes_top_cell.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     worksheet.write_string(0, 0, "Scroll down")?;
    /// #
    ///     // Freeze the top row only.
    ///     worksheet.set_freeze_panes(1, 0)?;
    ///
    ///     // Pre-scroll to the row 20.
    ///     worksheet.set_freeze_panes_top_cell(19, 0)?;
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_freeze_panes_top_cell.png">
    ///
    pub fn set_freeze_panes_top_cell(
        &mut self,
        row: RowNum,
        col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and col are in the allowed range.
        if !self.check_dimensions_only(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        self.panes.top_cell = (row, col);
        Ok(self)
    }

    /// Set the printed page header caption.
    ///
    /// The `set_header()` method can be used to set the header for a worksheet.
    ///
    /// Headers and footers are generated using a string which is a combination
    /// of plain text and optional control characters.
    ///
    /// The available control characters are:
    ///
    /// | Control              | Category      | Description           |
    /// | -------------------- | ------------- | --------------------- |
    /// | `&L`                 | Alignment     | Left                  |
    /// | `&C`                 |               | Center                |
    /// | `&R`                 |               | Right                 |
    /// | `&[Page]`  or `&P`   | Information   | Page number           |
    /// | `&[Pages]` or `&N`   |               | Total number of pages |
    /// | `&[Date]`  or `&D`   |               | Date                  |
    /// | `&[Time]`  or `&T`   |               | Time                  |
    /// | `&[File]`  or `&F`   |               | File name             |
    /// | `&[Tab]`   or `&A`   |               | Worksheet name        |
    /// | `&[Path]`  or `&Z`   |               | Workbook path         |
    /// | `&fontsize`          | Font          | Font size             |
    /// | `&"font,style"`      |               | Font name and style   |
    /// | `&U`                 |               | Single underline      |
    /// | `&E`                 |               | Double underline      |
    /// | `&S`                 |               | Strikethrough         |
    /// | `&X`                 |               | Superscript           |
    /// | `&Y`                 |               | Subscript             |
    /// | `&[Picture]` or `&G` | Images        | Picture/image         |
    /// | `&&`                 | Miscellaneous | Literal ampersand &   |
    ///
    /// Some of the placeholder variables have a long version like `&[Page]` and
    /// a short version like `&P`. The longer version is displayed in the Excel
    /// interface but the shorter version is the way that it is stored in the
    /// file format. Either version is okay since `rust_xlsxwriter` will
    /// translate as required.
    ///
    /// Headers and footers have 3 edit areas to the left, center and right.
    /// Text can be aligned to these areas by prefixing the text with the
    /// control characters `&L`, `&C` and `&R`.
    ///
    /// For example:
    ///
    /// ```text
    /// worksheet.set_header("&LHello");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// | Hello                                                         |
    /// |                                                               |
    ///
    /// worksheet.set_header("&CHello");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// |                          Hello                                |
    /// |                                                               |
    ///
    /// worksheet.set_header("&RHello");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// |                                                         Hello |
    /// |                                                               |
    /// ```
    ///
    /// You can also have text in each of the alignment areas:
    ///
    /// ```text
    /// worksheet.set_header("&LCiao&CBello&RCielo");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// | Ciao                     Bello                          Cielo |
    /// |                                                               |
    /// ```
    ///
    /// The information control characters act as variables/templates that Excel
    /// will update/expand as the workbook or worksheet changes.
    ///
    /// ```text
    /// worksheet.set_header("&CPage &[Page] of &[Pages]");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// |                        Page 1 of 6                            |
    /// |                                                               |
    /// ```
    ///
    /// Times and dates are in the user's default format:
    ///
    /// ```text
    /// worksheet.set_header("&CUpdated at &[Time]");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// |                    Updated at 12:30 PM                        |
    /// |                                                               |
    /// ```
    ///
    /// To include a single literal ampersand `&` in a header or footer you
    /// should use a double ampersand `&&`:
    ///
    /// ```text
    /// worksheet.set_header("&CCuriouser && Curiouser - Attorneys at Law");
    ///
    ///  ---------------------------------------------------------------
    /// |                                                               |
    /// |                   Curiouser & Curiouser                       |
    /// |                                                               |
    /// ```
    ///
    /// You can specify the font size of a section of the text by prefixing it
    /// with the control character `&n` where `n` is the font size:
    ///
    /// ```text
    /// worksheet1.set_header("&C&30Hello Big");
    /// worksheet2.set_header("&C&10Hello Small");
    /// ```
    ///
    /// You can specify the font of a section of the text by prefixing it with
    /// the control sequence `&"font,style"` where `fontname` is a font name
    /// such as Windows font descriptions: "Regular", "Italic", "Bold" or "Bold
    /// Italic": "Courier New" or "Times New Roman" and `style` is one of the
    /// standard Windows font descriptions like “Regular”, “Italic”, “Bold” or
    /// “Bold Italic”:
    ///
    /// ```text
    /// worksheet1.set_header(r#"&C&"Courier New,Italic"Hello"#);
    /// worksheet2.set_header(r#"&C&"Courier New,Bold Italic"Hello"#);
    /// worksheet3.set_header(r#"&C&"Times New Roman,Regular"Hello"#);
    /// ```
    ///
    /// It is possible to combine all of these features together to create
    /// complex headers and footers. If you set up a complex header in Excel you
    /// can transfer it to `rust_xlsxwriter` by inspecting the string in the
    /// Excel file. For example the following shows how unzip and grep the Excel
    /// XML sub-files on a Linux system. The example uses libxml's xmllint to
    /// format the XML for clarity:
    ///
    /// ```text
    /// $ unzip myfile.xlsm -d myfile
    /// $ xmllint --format `find myfile -name "*.xml" | xargs` | \
    ///     egrep "Header|Footer" | sed 's/&amp;/\&/g'
    ///
    ///  <headerFooter scaleWithDoc="0">
    ///    <oddHeader>&L&P</oddHeader>
    ///  </headerFooter>
    /// ```
    ///
    /// Note: Excel requires that the header or footer string be less than 256
    /// characters, including the control characters. Strings longer than this
    /// will not be written, and a warning will be output.
    ///
    /// # Arguments
    ///
    /// * `header` - The header string with optional control characters.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet header.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_header.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet.set_header("&CPage &P of &N");
    ///
    /// #     worksheet.write_string(0, 0, "Hello")?;
    /// #     worksheet.write_string(200, 0, "Hello")?;
    /// #     worksheet.set_view_page_layout();
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_header.png">
    ///
    pub fn set_header(&mut self, header: &str) -> &mut Worksheet {
        let header_copy = header
            .replace("&[Tab]", "&A")
            .replace("&[Date]", "&D")
            .replace("&[File]", "&F")
            .replace("&[Page]", "&P")
            .replace("&[Path]", "&Z")
            .replace("&[Time]", "&T")
            .replace("&[Pages]", "&N")
            .replace("&[Picture]", "&G");

        if header_copy.chars().count() > 255 {
            eprintln!("Header string exceeds Excel's limit of 255 characters.");
            return self;
        }

        self.header = header.to_string();
        self.page_setup_changed = true;
        self.head_footer_changed = true;
        self
    }

    /// Set the printed page footer caption.
    ///
    /// The `set_footer()` method can be used to set the footer for a worksheet.
    ///
    /// See the documentation for [`set_header()`](Worksheet::set_header()) for
    /// more details on the syntax of the header/footer string.
    ///
    /// # Arguments
    ///
    /// * `footer` - The footer string with optional control characters.
    ///
    pub fn set_footer(&mut self, footer: &str) -> &mut Worksheet {
        let footer_copy = footer
            .replace("&[Tab]", "&A")
            .replace("&[Date]", "&D")
            .replace("&[File]", "&F")
            .replace("&[Page]", "&P")
            .replace("&[Path]", "&Z")
            .replace("&[Time]", "&T")
            .replace("&[Pages]", "&N")
            .replace("&[Picture]", "&G");

        if footer_copy.chars().count() > 255 {
            eprintln!("Footer string exceeds Excel's limit of 255 characters.");
            return self;
        }

        self.footer = footer.to_string();
        self.page_setup_changed = true;
        self.head_footer_changed = true;
        self
    }

    /// Insert an image in a worksheet header.
    ///
    /// Insert an image in a worksheet header in one of the 3 sections supported
    /// by Excel: Left, Center and Right. This needs to be preceded by a call to
    /// [worksheet.set_header()](Worksheet::set_header) where a corresponding
    /// `&[Picture]` element is added to the header formatting string such as
    /// `"&L&[Picture]"`.
    ///
    /// # Arguments
    ///
    /// * `position` - The image position as defined by the [HeaderImagePosition]
    ///   enum.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::ParameterError`] - Parameter error if there isn't a
    ///   corresponding `&[Picture]`/`&[G]` variable in the header string.
    ///
    /// # Examples
    ///
    /// The following example demonstrates adding a header image to a worksheet.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_header_image.rs
    /// #
    /// # use rust_xlsxwriter::{Image, Workbook, XlsxError, HeaderImagePosition};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    /// #     // Scale the image so it fits in the header.
    ///     let mut image = Image::new("examples/rust_logo.png")?;
    /// #     image.set_scale_height(0.5);
    /// #     image.set_scale_width(0.5);
    /// #
    ///     // Insert the watermark image in the header.
    ///     worksheet.set_header("&C&[Picture]");
    ///     worksheet.set_header_image(&image, HeaderImagePosition::Center)?;
    ///
    /// #     // Increase the top margin to 1.2 for clarity. The -1.0 values are ignored.
    /// #     worksheet.set_margins(-1.0, -1.0, 1.2, -1.0, -1.0, -1.0);
    /// #
    /// #     // Set Page View mode so the watermark is visible.
    /// #     worksheet.set_view_page_layout();
    /// #
    /// #     // Save the file to disk.
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_header_image.png">
    ///
    /// An example of adding a worksheet watermark image using the
    /// rust_xlsxwriter library. This is based on the method of putting an image
    /// in the worksheet header as suggested in the [Microsoft documentation].
    ///
    /// [Microsoft documentation]:
    ///     https://support.microsoft.com/en-us/office/add-a-watermark-in-excel-a372182a-d733-484e-825c-18ddf3edf009
    ///
    /// ```
    /// # // This code is available in examples/app_watermark.rs
    /// #
    /// # use rust_xlsxwriter::{Image, Workbook, XlsxError, HeaderImagePosition};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     // Create a new Excel file object.
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     let image = Image::new("examples/watermark.png")?;
    ///
    ///     // Insert the watermark image in the header.
    ///     worksheet.set_header("&C&[Picture]");
    ///     worksheet.set_header_image(&image, HeaderImagePosition::Center)?;
    /// #
    /// #     // Set Page View mode so the watermark is visible.
    /// #     worksheet.set_view_page_layout();
    /// #
    /// #     // Save the file to disk.
    /// #     workbook.save("watermark.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/app_watermark.png">
    ///
    pub fn set_header_image(
        &mut self,
        image: &Image,
        position: HeaderImagePosition,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check that there is a matching  &[Picture]/&[G] variable in the
        // header string.
        if !self.verify_header_footer_image(&self.header, &position) {
            let error = format!(
                "No &[Picture] or &[G] variable in header string: '{}' for position = '{:?}'",
                self.header, position
            );
            return Err(XlsxError::ParameterError(error));
        }

        let mut image = image.clone();
        image.header_position = position.clone();
        image.is_header = true;
        self.header_footer_images[position as usize] = Some(image);

        Ok(self)
    }

    /// Insert an image in a worksheet footer.
    ///
    /// See the documentation for
    /// [`set_header_image()`](Worksheet::set_header_image()) for more details.
    ///
    /// # Arguments
    ///
    /// * `position` - The image position as defined by the [HeaderImagePosition]
    ///   enum.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::ParameterError`] - Parameter error if there isn't a
    ///   corresponding `&[Picture]`/`&[G]` variable in the header string.
    ///
    pub fn set_footer_image(
        &mut self,
        image: &Image,
        position: HeaderImagePosition,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check that there is a matching  &[Picture]/&[G] variable in the
        // footer string.
        if !self.verify_header_footer_image(&self.footer, &position) {
            let error = format!(
                "No &[Picture] or &[G] variable in footer string: '{}' for position = '{:?}'",
                self.footer, position
            );
            return Err(XlsxError::ParameterError(error));
        }

        let mut image = image.clone();
        image.header_position = position.clone();
        image.is_header = false;
        self.header_footer_images[3 + position as usize] = Some(image);

        Ok(self)
    }

    /// Set the page setup option to scale the header/footer with the document.
    ///
    /// This option determines whether the headers and footers use the same
    /// scaling as the worksheet. This defaults to "on" in Excel.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is on by default.
    ///
    pub fn set_header_footer_scale_with_doc(&mut self, enable: bool) -> &mut Worksheet {
        self.header_footer_scale_with_doc = enable;

        if !enable {
            self.page_setup_changed = true;
            self.head_footer_changed = true;
        }

        self
    }

    /// Set the page setup option to align the header/footer with the margins.
    ///
    /// This option determines whether the headers and footers align with the
    /// left and right margins of the worksheet. This defaults to "on" in Excel.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is on by default.
    ///`
    pub fn set_header_footer_align_with_page(&mut self, enable: bool) -> &mut Worksheet {
        self.header_footer_align_with_page = enable;

        if !enable {
            self.page_setup_changed = true;
            self.head_footer_changed = true;
        }
        self
    }

    /// Set the page margins.
    ///
    /// The `set_margins()` method is used to set the margins of the worksheet
    /// when it is printed. The units are in inches. Specifying `-1.0` for any
    /// parameter will give the default Excel value. The defaults are shown
    /// below.
    ///
    /// # Arguments
    ///
    /// * `left` - Left margin in inches. Excel default is 0.7.
    /// * `right` - Right margin in inches. Excel default is 0.7.
    /// * `top` - Top margin in inches. Excel default is 0.75.
    /// * `bottom` - Bottom margin in inches. Excel default is 0.75.
    /// * `header` - Header margin in inches. Excel default is 0.3.
    /// * `footer` - Footer margin in inches. Excel default is 0.3.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the worksheet margins.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_margins.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet.set_margins(1.0, 1.25, 1.5, 1.75, 0.75, 0.25);
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_margins.png">
    ///
    pub fn set_margins(
        &mut self,
        left: f64,
        right: f64,
        top: f64,
        bottom: f64,
        header: f64,
        footer: f64,
    ) -> &mut Worksheet {
        if left >= 0.0 {
            self.margin_left = left;
            self.page_setup_changed = true;
        }
        if right >= 0.0 {
            self.margin_right = right;
            self.page_setup_changed = true;
        }
        if top >= 0.0 {
            self.margin_top = top;
            self.page_setup_changed = true;
        }
        if bottom >= 0.0 {
            self.margin_bottom = bottom;
            self.page_setup_changed = true;
        }
        if header >= 0.0 {
            self.margin_header = header;
            self.page_setup_changed = true;
        }
        if footer >= 0.0 {
            self.margin_footer = footer;
            self.page_setup_changed = true;
        }

        self
    }

    /// Set the first page number when printing.
    ///
    /// The `set_print_first_page_number()` method is used to set the page
    /// number of the first page when the worksheet is printed out. This option
    /// will only have and effect if you have a header/footer with the `&[Page]`
    /// control character, see [`set_header()`](Worksheet::set_header()).
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `page_number` - The page number of the first printed page.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the page number on the printed
    /// page.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_print_first_page_number.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     worksheet.set_header("&CPage &P of &N");
    ///     worksheet.set_print_first_page_number(2);
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    pub fn set_print_first_page_number(&mut self, page_number: u16) -> &mut Worksheet {
        self.first_page_number = page_number;
        self.page_setup_changed = true;
        self
    }

    /// Set the page setup option to set the print scale.
    ///
    /// Set the scale factor of the printed page, in the range 10 <= scale <=
    /// 400.
    ///
    /// The default scale factor is 100. The `set_print_scale()` method
    /// does not affect the scale of the visible page in Excel. For that you
    /// should use [`set_zoom()`](Worksheet::set_zoom).
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `scale` - The print scale factor.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the scale of the worksheet page
    /// when printed.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_print_scale.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Scale the printed worksheet to 50%.
    ///     worksheet.set_print_scale(50);
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    pub fn set_print_scale(&mut self, scale: u16) -> &mut Worksheet {
        if !(10..=400).contains(&scale) {
            eprintln!("Scale factor {scale} outside Excel range: 10 <= zoom <= 400.");
            return self;
        }

        // This property is mutually exclusive with fit to page.
        self.fit_to_page = false;

        self.print_scale = scale;
        self.page_setup_changed = true;
        self
    }

    /// Fit the printed area to a specific number of pages both vertically and
    /// horizontally.
    ///
    /// The `set_print_fit_to_pages()` method is used to fit the printed area to
    /// a specific number of pages both vertically and horizontally. If the
    /// printed area exceeds the specified number of pages it will be scaled
    /// down to fit. This ensures that the printed area will always appear on
    /// the specified number of pages even if the page size or margins change:
    ///
    /// ```text
    ///     worksheet1.set_print_fit_to_pages(1, 1); // Fit to 1x1 pages.
    ///     worksheet2.set_print_fit_to_pages(2, 1); // Fit to 2x1 pages.
    ///     worksheet3.set_print_fit_to_pages(1, 2); // Fit to 1x2 pages.
    /// ```
    ///
    /// The print area can be defined using the `set_print_area()` method.
    ///
    /// A common requirement is to fit the printed output to `n` pages wide but
    /// have the height be as long as necessary. To achieve this set the
    /// `height` to 0, see the example below.
    ///
    /// **Notes**:
    ///
    /// - The `set_print_fit_to_pages()` will override any manual page breaks
    ///   that are defined in the worksheet.
    ///
    /// - When using `set_print_fit_to_pages()` it may also be required to set
    ///   the printer paper size using
    ///   [`set_paper_size()`](Worksheet::set_paper_size) or else Excel will
    ///   default to "US Letter".
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `width` - Number of pages horizontally.
    /// * `height` - Number of pages vertically.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the scale of the worksheet to fit
    /// a defined number of pages vertically and horizontally. This example shows a
    /// common use case which is to fit the printed output to 1 page wide but have
    /// the height be as long as necessary.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_print_fit_to_pages.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Set the printed output to fit 1 page wide and as long as necessary.
    ///     worksheet.set_print_fit_to_pages(1, 0);
    ///
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_print_fit_to_pages.png">
    ///
    pub fn set_print_fit_to_pages(&mut self, width: u16, height: u16) -> &mut Worksheet {
        self.fit_width = width;
        self.fit_height = height;

        // This property is mutually exclusive with print scale.
        self.print_scale = 100;

        self.fit_to_page = true;
        self.page_setup_changed = true;
        self
    }

    /// Center the printed page horizontally.
    ///
    /// Center the worksheet data horizontally between the margins on the
    /// printed page
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_print_center_horizontally(&mut self, enable: bool) -> &mut Worksheet {
        self.center_horizontally = enable;

        if enable {
            self.print_options_changed = true;
            self.page_setup_changed = true;
        }
        self
    }

    /// Center the printed page vertically.
    ///
    /// Center the worksheet data vertically between the margins on the
    /// printed page
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_print_center_vertically(&mut self, enable: bool) -> &mut Worksheet {
        self.center_vertically = enable;

        if enable {
            self.print_options_changed = true;
            self.page_setup_changed = true;
        }
        self
    }

    /// Set the page setup option to turn on printed gridlines.
    ///
    /// The `set_print_gridlines()` method is use to turn on/off gridlines on
    /// the printed pages. It is off by default.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_print_gridlines(&mut self, enable: bool) -> &mut Worksheet {
        self.print_gridlines = enable;

        if enable {
            self.print_options_changed = true;
            self.page_setup_changed = true;
        }
        self
    }

    /// Set the page setup option to print in black and white.
    ///
    /// This `set_print_black_and_white()` method can be used to force printing
    /// in black and white only. It is off by default.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_print_black_and_white(&mut self, enable: bool) -> &mut Worksheet {
        self.print_black_and_white = enable;

        if enable {
            self.page_setup_changed = true;
        }
        self
    }

    /// Set the page setup option to print in draft quality.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page Setup].
    ///
    /// [Worksheet - Page Setup]: https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_print_draft(&mut self, enable: bool) -> &mut Worksheet {
        self.print_draft = enable;

        if enable {
            self.page_setup_changed = true;
        }
        self
    }

    /// Set the page setup option to print the row and column headers on the
    /// printed page.
    ///
    /// The `set_print_headings()` method turns on the row and column headers
    /// when printing a worksheet. This option is off by default.
    ///
    /// See also the `rust_xlsxwriter` documentation on [Worksheet - Page
    /// Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `enable` - Turn the property on/off. It is off by default.
    ///
    pub fn set_print_headings(&mut self, enable: bool) -> &mut Worksheet {
        self.print_headings = enable;

        if enable {
            self.print_options_changed = true;
            self.page_setup_changed = true;
        }
        self
    }

    /// Set the print area for the worksheet.
    ///
    /// This method is used to specify the area of the worksheet that will be
    /// printed.
    ///
    /// In order to specify an entire row or column range such as `1:20` or
    /// `A:H` you must specify the corresponding maximum column or row range.
    /// For example:
    ///
    /// - `(0, 0, 31, 16_383) == 1:32`.
    /// - `(0, 0, 1_048_575, 12) == A:M`.
    ///
    /// In these examples 16_383 is the maximum column and 1_048_575 is the
    /// maximum row (zero indexed).
    ///
    /// See also the example below and the `rust_xlsxwriter` documentation on
    /// [Worksheet - Page Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (All zero indexed.)
    /// * `first_col` - The first row of the range.
    /// * `last_row` - The last row of the range.
    /// * `last_col` - The last row of the range.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row or column is larger
    ///   than the last row or column.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the print area for several
    /// worksheets.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_print_area.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = workbook.add_worksheet();
    ///     // Set the print area to "A1:M32"
    ///     worksheet1.set_print_area(0, 0, 31, 12)?;
    ///
    ///     let worksheet2 = workbook.add_worksheet();
    ///     // Set the print area to "1:32"
    ///     worksheet2.set_print_area(0, 0, 31, 16_383)?;
    ///
    ///     let worksheet3 = workbook.add_worksheet();
    ///     // Set the print area to "A:M"
    ///     worksheet3.set_print_area(0, 0, 1_048_575, 12)?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file, page setup dialog for worksheet1:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_set_print_area.png">
    ///
    pub fn set_print_area(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows and cols are in the allowed range.
        if !self.check_dimensions_only(first_row, first_col)
            || !self.check_dimensions_only(last_row, last_col)
        {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_row > last_row || first_col > last_col {
            return Err(XlsxError::RowColumnOrderError);
        }

        // The print range is the entire worksheet, therefore it is the same
        // as the default, so we can ignore it.
        if first_row == 0 && first_col == 0 && last_row == ROW_MAX - 1 && last_col == COL_MAX - 1 {
            return Ok(self);
        }

        // Store the defined name information.
        self.print_area_defined_name.in_use = true;
        self.print_area_defined_name.name_type = DefinedNameType::PrintArea;
        self.print_area_defined_name.first_row = first_row;
        self.print_area_defined_name.first_col = first_col;
        self.print_area_defined_name.last_row = last_row;
        self.print_area_defined_name.last_col = last_col;

        self.page_setup_changed = true;
        Ok(self)
    }

    /// Set the number of rows to repeat at the top of each printed page.
    ///
    /// For large Excel documents it is often desirable to have the first row or
    /// rows of the worksheet print out at the top of each page.
    ///
    /// See the example below and the `rust_xlsxwriter` documentation on
    /// [Worksheet - Page Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `first_row` - The first row of the range. (Zero indexed.)
    /// * `last_row` - The last row of the range.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row larger than the last
    ///   row.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the rows to repeat on each
    /// printed page.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_repeat_rows.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = workbook.add_worksheet();
    ///     // Repeat the first row in the printed output.
    ///     worksheet1.set_repeat_rows(0, 0)?;
    ///
    ///     let worksheet2 = workbook.add_worksheet();
    ///     // Repeat the first 2 rows in the printed output.
    ///     worksheet2.set_repeat_rows(0, 1)?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file, page setup dialog for worksheet2:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_repeat_rows.png">
    ///
    pub fn set_repeat_rows(
        &mut self,
        first_row: RowNum,
        last_row: RowNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows are in the allowed range.
        if !self.check_dimensions_only(first_row, 0) || !self.check_dimensions_only(last_row, 0) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_row > last_row {
            return Err(XlsxError::RowColumnOrderError);
        }

        // Store the range data.
        self.repeat_row_cols_defined_name.in_use = true;
        self.repeat_row_cols_defined_name.name_type = DefinedNameType::PrintTitles;
        self.repeat_row_cols_defined_name.first_row = first_row;
        self.repeat_row_cols_defined_name.last_row = last_row;

        self.page_setup_changed = true;
        Ok(self)
    }

    /// Set the columns to repeat at the left hand side of each printed page.
    ///
    /// For large Excel documents it is often desirable to have the first column
    /// or columns of the worksheet print out at the left hand side of each
    /// page.
    ///
    /// See the example below and the `rust_xlsxwriter` documentation on
    /// [Worksheet - Page Setup].
    ///
    /// [Worksheet - Page Setup]:
    ///     https://rustxlsxwriter.github.io/worksheet/page_setup.html
    ///
    /// # Arguments
    ///
    /// * `first_col` - The first row of the range. (Zero indexed.)
    /// * `last_col` - The last row of the range.
    ///
    /// # Errors
    ///
    /// * [`XlsxError::RowColumnLimitError`] - Row or column exceeds Excel's
    ///   worksheet limits.
    /// * [`XlsxError::RowColumnOrderError`] - First row or column is larger
    ///   than the last row or column.
    ///
    /// # Examples
    ///
    /// The following example demonstrates setting the columns to repeat on each
    /// printed page.
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_set_repeat_columns.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    ///     let worksheet1 = workbook.add_worksheet();
    ///     // Repeat the first column in the printed output.
    ///     worksheet1.set_repeat_columns(0, 0)?;
    ///
    ///     let worksheet2 = workbook.add_worksheet();
    ///     // Repeat the first 2 columns in the printed output.
    ///     worksheet2.set_repeat_columns(0, 1)?;
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file, page setup dialog for worksheet2:
    ///
    /// <img src="https://rustxlsxwriter.github.io/images/worksheet_set_repeat_columns.png">
    ///
    pub fn set_repeat_columns(
        &mut self,
        first_col: ColNum,
        last_col: ColNum,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check columns are in the allowed range.
        if !self.check_dimensions_only(0, first_col) || !self.check_dimensions_only(0, last_col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_col > last_col {
            return Err(XlsxError::RowColumnOrderError);
        }

        // Store the defined name information.
        self.repeat_row_cols_defined_name.in_use = true;
        self.repeat_row_cols_defined_name.name_type = DefinedNameType::PrintTitles;
        self.repeat_row_cols_defined_name.first_col = first_col;
        self.repeat_row_cols_defined_name.last_col = last_col;

        self.page_setup_changed = true;
        Ok(self)
    }

    /// Autofit the worksheet column widths, approximately.
    ///
    /// Simulate column auto-fitting based on the data in the worksheet columns.
    ///
    /// There is no option in the xlsx file format that can be used to say
    /// "autofit columns on loading". Auto-fitting of columns is something that
    /// Excel does at runtime when it has access to all of the worksheet
    /// information as well as the Windows functions for calculating display
    /// areas based on fonts and formatting.
    ///
    /// As such `worksheet.autofit()` simulates this behavior by calculating
    /// string widths using metrics taken from Excel. This isn't perfect but for
    /// most cases it should be sufficient and if not you can set your own
    /// widths, see below.
    ///
    /// The `autofit()` method ignores columns that already have an explicit
    /// column width set via
    /// [`set_column_width()`](Worksheet::set_column_width()) or
    /// [`set_column_width_pixels()`](Worksheet::set_column_width_pixels()) if
    /// it is greater than the calculate maximum width. Alternatively, calling
    /// these methods after `autofit()` will override the autofit value.
    ///
    /// **Note**, `autofit()` iterates through all the cells in a worksheet that
    /// have been populated with data and performs a length calculation on each
    /// one, so it can have a performance overhead for larger worksheets.
    ///
    /// # Examples
    ///
    /// The following example demonstrates auto-fitting the worksheet column
    /// widths based on the data in the columns. See all the [Autofitting
    /// Columns] example in the user guide/examples directory.
    ///
    /// [Autofitting Columns]:
    ///     https://rustxlsxwriter.github.io/examples/autofit.html
    ///
    /// ```
    /// # // This code is available in examples/doc_worksheet_autofit.rs
    /// #
    /// # use rust_xlsxwriter::{Workbook, XlsxError};
    /// #
    /// # fn main() -> Result<(), XlsxError> {
    /// #     let mut workbook = Workbook::new();
    /// #
    /// #     // Add a worksheet to the workbook.
    /// #     let worksheet = workbook.add_worksheet();
    /// #
    ///     // Add some data
    ///     worksheet.write_string(0, 0, "Hello")?;
    ///     worksheet.write_string(0, 1, "Hello")?;
    ///     worksheet.write_string(1, 1, "Hello World")?;
    ///     worksheet.write_number(0, 2, 123)?;
    ///     worksheet.write_number(0, 3, 123456)?;
    ///
    ///     // Autofit the columns.
    ///     worksheet.autofit();
    /// #
    /// #     workbook.save("worksheet.xlsx")?;
    /// #
    /// #     Ok(())
    /// # }
    /// ```
    ///
    /// Output file:
    ///
    /// <img
    /// src="https://rustxlsxwriter.github.io/images/worksheet_autofit.png">
    ///
    pub fn autofit(&mut self) -> &mut Worksheet {
        let mut max_widths: HashMap<ColNum, u16> = HashMap::new();

        // Iterate over all of the data in the worksheet and find the max data
        // width for each column.
        for row_num in self.dimensions.first_row..=self.dimensions.last_row {
            if let Some(columns) = self.table.get(&row_num) {
                for col_num in self.dimensions.first_col..=self.dimensions.last_col {
                    if let Some(cell) = columns.get(&col_num) {
                        let pixel_width = match cell {
                            // For strings we do a calculation based on
                            // character widths taken from Excel. For rich
                            // strings we use the unformatted string. We also
                            // split multi-line strings and handle each part
                            // separately.
                            CellType::String { string, .. }
                            | CellType::RichString {
                                string: _,
                                xf_index: _,
                                raw_string: string,
                            } => {
                                if !string.contains('\n') {
                                    utility::pixel_width(string)
                                } else {
                                    let mut max = 0;
                                    for segment in string.split('\n') {
                                        let length = utility::pixel_width(segment);
                                        max = cmp::max(max, length);
                                    }
                                    max
                                }
                            }

                            // For numbers we use a workaround/optimization
                            // since digits all have a pixel width of 7. This
                            // gives a slightly greater width for the decimal
                            // place and minus sign but only by a few pixels and
                            // over-estimation is okay.
                            CellType::Number { number, .. } => 7 * number.to_string().len() as u16,

                            // For Boolean types we use the Excel standard
                            // widths for TRUE and FALSE.
                            CellType::Boolean { boolean, .. } => {
                                if *boolean {
                                    31
                                } else {
                                    36
                                }
                            }

                            // For formulas we autofit the result of the formula
                            // if it has a non-zero/default value.
                            CellType::Formula {
                                formula: _,
                                xf_index: _,
                                result,
                            }
                            | CellType::ArrayFormula {
                                formula: _,
                                xf_index: _,
                                result,
                                ..
                            } => {
                                if result == "0" || result.is_empty() {
                                    0
                                } else {
                                    utility::pixel_width(result)
                                }
                            }

                            // Datetimes are just numbers but they also have an
                            // Excel format. It isn't feasible to parse the
                            // number format to get the actual string width for
                            // all format types so we use a width based on the
                            // Excel's default format: mm/dd/yyyy.
                            CellType::DateTime { .. } => 68,

                            // Ignore blank cells, like Excel.
                            CellType::Blank { .. } => 0,
                        };

                        // Update the max column width.
                        if pixel_width > 0 {
                            match max_widths.get_mut(&col_num) {
                                // Update the max for the column.
                                Some(max) => {
                                    if pixel_width > *max {
                                        *max = pixel_width
                                    }
                                }
                                None => {
                                    // Add a new column entry and maximum.
                                    max_widths.insert(col_num, pixel_width);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Set the max character width for each column.
        for (col, pixels) in max_widths.iter() {
            let width = self.pixels_to_width(*pixels + 7);
            self.store_column_width(*col, width, true);
        }

        self
    }

    // -----------------------------------------------------------------------
    // Crate level helper methods.
    // -----------------------------------------------------------------------

    // Hide any rows in the autofilter range that don't match the autofilter
    // conditions, like Excel does at runtime.
    pub(crate) fn hide_autofilter_rows(&mut self) {
        if self.filter_conditions.is_empty() || self.filter_automatic_off {
            return;
        }

        // Get the range that the autofilter applies to.
        let filter_columns: Vec<ColNum> = self.filter_conditions.keys().cloned().collect();
        let first_row = self.autofilter_defined_name.first_row + 1; // Skip header.
        let last_row = self.autofilter_defined_name.last_row;

        for col_num in filter_columns {
            // Iterate through each column filter conditions.
            let filter_condition = self.filter_conditions.get(&col_num).unwrap().clone();
            for row_num in first_row..=last_row {
                if filter_condition.is_list_filter {
                    // Handle list filters.
                    if !self.row_matches_list_filter(row_num, col_num, &filter_condition) {
                        self.set_row_hidden(row_num).unwrap();
                    }
                } else {
                    // Handle custom filters.
                    if !self.row_matches_custom_filters(row_num, col_num, &filter_condition) {
                        self.set_row_hidden(row_num).unwrap();
                    }
                }
            }
        }
    }

    // Check if the data in a cell matches one of the values in the list of
    // filter conditions (which in the list filter case is a list of strings or
    // number values).
    //
    // Excel trims leading and trailing space and then does a lowercase
    // comparison. It also matches numbers against "numbers stored as strings".
    // It also treats "blanks" as empty cells but also any string that is
    // composed of whitespace. See the test cases for examples. We try to match
    // all these conditions.
    fn row_matches_list_filter(
        &self,
        row_num: RowNum,
        col_num: ColNum,
        filter_condition: &FilterCondition,
    ) -> bool {
        let mut has_cell_data = false;

        if let Some(columns) = self.table.get(&row_num) {
            if let Some(cell) = columns.get(&col_num) {
                has_cell_data = true;

                match cell {
                    CellType::String { string, .. }
                    | CellType::RichString {
                        string: _,
                        xf_index: _,
                        raw_string: string,
                    } => {
                        let cell_string = string.clone().to_lowercase().trim().to_string();

                        for filter in &filter_condition.list {
                            if cell_string == filter.string.to_lowercase().trim() {
                                return true;
                            }
                        }

                        if filter_condition.should_match_blanks && cell_string.is_empty() {
                            return true;
                        }
                    }
                    CellType::Number { number, .. } => {
                        for filter in &filter_condition.list {
                            if filter.data_type == FilterDataType::Number
                                && number == &filter.number
                            {
                                return true;
                            }
                        }
                    }
                    CellType::Blank { .. } => {
                        if filter_condition.should_match_blanks {
                            return true;
                        }
                    }
                    // We don't currently try to handle matching any other data types.
                    _ => {}
                };
            }
        }

        // If there is no cell data then that qualifies as Blanks in Excel.
        if !has_cell_data && filter_condition.should_match_blanks {
            return true;
        }

        // If none of the conditions match then we return false and hide the row.
        false
    }

    // Check if the data in a cell matches one of the conditions and values is a
    // custom filter. Excel allows 1 or 2 custom filters. We check for each
    // filter and evaluate the result(s) with the user defined and/or condition.
    fn row_matches_custom_filters(
        &self,
        row_num: RowNum,
        col_num: ColNum,
        filter_condition: &FilterCondition,
    ) -> bool {
        let condition1;
        let condition2;

        if let Some(data) = &filter_condition.custom1 {
            condition1 = self.row_matches_custom_filter(row_num, col_num, data);
        } else {
            condition1 = false;
        }

        if let Some(data) = &filter_condition.custom2 {
            condition2 = self.row_matches_custom_filter(row_num, col_num, data);
        } else {
            return condition1;
        }

        if filter_condition.apply_logical_or {
            condition1 || condition2
        } else {
            condition1 && condition2
        }
    }

    // Check if the data in a cell matches one custom filter.
    //
    // Excel trims leading and trailing space and then does a lowercase
    // comparison. It also matches numbers against "numbers stored as strings".
    // It also applies the comparison operators to strings. However, it doesn't
    // apply the string criteria (like contains()) to numbers (unless they are
    // stored as strings).
    fn row_matches_custom_filter(
        &self,
        row_num: RowNum,
        col_num: ColNum,
        filter: &FilterData,
    ) -> bool {
        if let Some(columns) = self.table.get(&row_num) {
            if let Some(cell) = columns.get(&col_num) {
                match cell {
                    CellType::String { string, .. }
                    | CellType::RichString {
                        string: _,
                        xf_index: _,
                        raw_string: string,
                    } => {
                        let cell_string = string.clone().to_lowercase().trim().to_string();
                        let filter_string = filter.string.to_lowercase().trim().to_string();

                        match filter.criteria {
                            FilterCriteria::EqualTo => return cell_string == filter_string,
                            FilterCriteria::NotEqualTo => return cell_string != filter_string,
                            FilterCriteria::LessThan => return cell_string < filter_string,
                            FilterCriteria::GreaterThan => return cell_string > filter_string,
                            FilterCriteria::LessThanOrEqualTo => {
                                return cell_string <= filter_string
                            }
                            FilterCriteria::GreaterThanOrEqualTo => {
                                return cell_string >= filter_string
                            }
                            FilterCriteria::EndsWith => {
                                return cell_string.ends_with(&filter_string)
                            }
                            FilterCriteria::DoesNotEndWith => {
                                return !cell_string.ends_with(&filter_string)
                            }
                            FilterCriteria::BeginsWith => {
                                return cell_string.starts_with(&filter_string)
                            }
                            FilterCriteria::DoesNotBeginWith => {
                                return !cell_string.starts_with(&filter_string)
                            }
                            FilterCriteria::Contains => {
                                return cell_string.contains(&filter_string)
                            }
                            FilterCriteria::DoesNotContain => {
                                return !cell_string.contains(&filter_string)
                            }
                        }
                    }
                    CellType::Number { number, .. } => {
                        if filter.data_type == FilterDataType::Number {
                            match filter.criteria {
                                FilterCriteria::EqualTo => return *number == filter.number,
                                FilterCriteria::LessThan => return *number < filter.number,
                                FilterCriteria::NotEqualTo => return *number != filter.number,
                                FilterCriteria::GreaterThan => return *number > filter.number,
                                FilterCriteria::LessThanOrEqualTo => {
                                    return *number <= filter.number
                                }
                                FilterCriteria::GreaterThanOrEqualTo => {
                                    return *number >= filter.number
                                }
                                _ => {}
                            }
                        }
                    }
                    CellType::Blank { .. } => {
                        // We need to handle "match non-blanks" as a special condition.
                        // Excel converts this to a custom filter of `!= " "`.
                        if filter.criteria == FilterCriteria::NotEqualTo && filter.string == " " {
                            return false;
                        }
                    }
                    _ => {
                        // Any existing non-blank cell should match the "non-blanks" criteria
                        // explained above.
                        if filter.criteria == FilterCriteria::NotEqualTo && filter.string == " " {
                            return true;
                        }
                    }
                };
            }
        }

        false
    }

    // Process pagebreaks to sort them, remove duplicates and check the number
    // is within the Excel limit.
    pub(crate) fn process_pagebreaks(&mut self, breaks: &[u32]) -> Result<Vec<u32>, XlsxError> {
        let unique_breaks: HashSet<u32> = breaks.iter().copied().collect();
        let mut breaks: Vec<u32> = unique_breaks.into_iter().collect();
        breaks.sort();

        // Remove invalid 0 row/col.
        if breaks[0] == 0 {
            breaks.remove(0);
        }

        // The Excel 2007 specification says that the maximum number of page
        // breaks is 1026. However, in practice it is actually 1023.
        if breaks.len() > 1023 {
            let error =
                "Maximum number of horizontal or vertical pagebreaks allowed by Excel is 1023"
                    .to_string();
            return Err(XlsxError::ParameterError(error));
        }

        Ok(breaks)
    }

    // Store a number cell in the worksheet data table structure.
    fn store_number(
        &mut self,
        row: RowNum,
        col: ColNum,
        number: f64,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        self.store_number_type(row, col, number, format, false)
    }

    // Store a datetime cell in the worksheet data table structure.
    fn store_datetime(
        &mut self,
        row: RowNum,
        col: ColNum,
        number: f64,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        self.store_number_type(row, col, number, format, true)
    }

    // Store a number/datetime cell in the worksheet data table structure.
    fn store_number_type(
        &mut self,
        row: RowNum,
        col: ColNum,
        number: f64,
        format: Option<&Format>,
        is_datetime: bool,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and col are in the allowed range.
        if !self.check_dimensions(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Excel doesn't have a NAN type/value so write a string instead.
        if number.is_nan() {
            return self.store_string(row, col, "#NUM!", None);
        }

        // Excel doesn't have an Infinity type/value so write a string instead.
        if number.is_infinite() {
            self.store_string(row, col, "#DIV/0", None)?;
        }

        // Get the index of the format object, if any.
        let xf_index = match format {
            Some(format) => self.format_index(format),
            None => 0,
        };

        // Create the appropriate cell type to hold the data.
        let cell = if is_datetime {
            CellType::DateTime { number, xf_index }
        } else {
            CellType::Number { number, xf_index }
        };

        self.insert_cell(row, col, cell);

        Ok(self)
    }

    // Store a string cell in the worksheet data table structure.
    fn store_string(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Empty strings are ignored by Excel unless they have a format in which
        // case they are treated as a blank cell.
        if string.is_empty() {
            match format {
                Some(format) => return self.write_blank(row, col, format),
                None => return Ok(self),
            };
        }

        // Check row and col are in the allowed range.
        if !self.check_dimensions(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        //  Check that the string is < Excel limit of 32767 chars.
        if string.chars().count() > MAX_STRING_LEN {
            return Err(XlsxError::MaxStringLengthExceeded);
        }

        // Get the index of the format object, if any.
        let xf_index = match format {
            Some(format) => self.format_index(format),
            None => 0,
        };

        // Create the appropriate cell type to hold the data.
        let cell = CellType::String {
            string: string.to_string(),
            xf_index,
        };

        self.insert_cell(row, col, cell);
        self.uses_string_table = true;

        Ok(self)
    }

    // Store a rich string cell in the worksheet data table structure.
    fn store_rich_string(
        &mut self,
        row: RowNum,
        col: ColNum,
        string: &str,
        raw_string: &str,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Empty strings are ignored by Excel unless they have a format in which
        // case they are treated as a blank cell.
        if string.is_empty() {
            match format {
                Some(format) => return self.write_blank(row, col, format),
                None => return Ok(self),
            };
        }

        // Check row and col are in the allowed range.
        if !self.check_dimensions(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        //  Check that the string is < Excel limit of 32767 chars.
        if string.chars().count() > MAX_STRING_LEN {
            return Err(XlsxError::MaxStringLengthExceeded);
        }

        // Get the index of the format object, if any.
        let xf_index = match format {
            Some(format) => self.format_index(format),
            None => 0,
        };

        // Create the appropriate cell type to hold the data.
        let cell = CellType::RichString {
            string: string.to_string(),
            xf_index,
            raw_string: raw_string.to_string(),
        };

        self.insert_cell(row, col, cell);
        self.uses_string_table = true;

        Ok(self)
    }

    // Store a formula cell in the worksheet data table structure.
    fn store_formula(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Transfer to dynamic formula handling function.
        if is_dynamic_function(formula) {
            return self.store_array_formula(row, col, row, col, formula, None, true);
        }

        // Check row and col are in the allowed range.
        if !self.check_dimensions(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Get the index of the format object, if any.
        let xf_index = match format {
            Some(format) => self.format_index(format),
            None => 0,
        };

        let formula = prepare_formula(formula, self.use_future_functions);

        // Create the appropriate cell type to hold the data.
        let cell = CellType::Formula {
            formula,
            xf_index,
            result: self.default_result.clone(),
        };

        self.insert_cell(row, col, cell);

        Ok(self)
    }

    // Store an array formula cell in the worksheet data table structure.
    #[allow(clippy::too_many_arguments)]
    fn store_array_formula(
        &mut self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
        formula: &str,
        format: Option<&Format>,
        is_dynamic: bool,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check rows and cols are in the allowed range.
        if !self.check_dimensions(first_row, first_col)
            || !self.check_dimensions(last_row, last_col)
        {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Check order of first/last values.
        if first_row > last_row || first_col > last_col {
            return Err(XlsxError::RowColumnOrderError);
        }

        // Get the index of the format object, if any.
        let xf_index = match format {
            Some(format) => self.format_index(format),
            None => 0,
        };

        let formula = prepare_formula(formula, self.use_future_functions);

        // Create the array range reference.
        let range = utility::cell_range(first_row, first_col, last_row, last_col);

        // Check for a dynamic function in a standard static array formula.
        let mut is_dynamic = is_dynamic;
        if !is_dynamic && is_dynamic_function(&formula) {
            is_dynamic = true;
        }

        if is_dynamic {
            self.has_dynamic_arrays = true;
        }

        // Create the appropriate cell type to hold the data.
        let cell = CellType::ArrayFormula {
            formula,
            xf_index,
            result: self.default_result.clone(),
            is_dynamic,
            range,
        };

        self.insert_cell(first_row, first_col, cell);

        // Pad out the rest of the area with formatted zeroes.
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                if !(row == first_row && col == first_col) {
                    match format {
                        Some(format) => self.write_number_with_format(row, col, 0, format).unwrap(),
                        None => self.write_number(row, col, 0).unwrap(),
                    };
                }
            }
        }

        Ok(self)
    }

    // Store a blank cell in the worksheet data table structure.
    fn store_blank(
        &mut self,
        row: RowNum,
        col: ColNum,
        format: &Format,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and col are in the allowed range.
        if !self.check_dimensions(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Get the index of the format object.
        let xf_index = self.format_index(format);

        // Create the appropriate cell type to hold the data.
        let cell = CellType::Blank { xf_index };

        self.insert_cell(row, col, cell);

        Ok(self)
    }

    // Store a boolean cell in the worksheet data table structure.
    fn store_boolean(
        &mut self,
        row: RowNum,
        col: ColNum,
        boolean: bool,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        // Check row and col are in the allowed range.
        if !self.check_dimensions(row, col) {
            return Err(XlsxError::RowColumnLimitError);
        }

        // Get the index of the format object, if any.
        let xf_index = match format {
            Some(format) => self.format_index(format),
            None => 0,
        };

        // Create the appropriate cell type to hold the data.
        let cell = CellType::Boolean { boolean, xf_index };

        self.insert_cell(row, col, cell);

        Ok(self)
    }

    // Store a url and associated properties. Urls in Excel are handled in a
    // number of ways: they are written as a string similar to write_string_with_format(),
    // they are written in the <hyperlinks> element within the worksheet, and
    // they are referenced in the worksheet.rels file.
    fn store_url(
        &mut self,
        row: RowNum,
        col: ColNum,
        url: &str,
        text: &str,
        tip: &str,
        format: Option<&Format>,
    ) -> Result<&mut Worksheet, XlsxError> {
        let hyperlink = Hyperlink::new(url, text, tip)?;

        match format {
            Some(format) => self.write_string_with_format(row, col, &hyperlink.text, format)?,
            None => {
                let hyperlink_format = Format::new().set_hyperlink();
                self.write_string_with_format(row, col, &hyperlink.text, &hyperlink_format)?
            }
        };

        self.hyperlinks.insert((row, col), hyperlink);

        Ok(self)
    }

    // A rich string is handled in Excel like any other shared string except
    // that it has inline font markup within the string. To generate the
    // required font xml we use an instance of the Style struct.
    fn get_rich_string(
        &mut self,
        segments: &[(&Format, &str)],
    ) -> Result<(String, String), XlsxError> {
        // Check that there is at least one segment tuple.
        if segments.is_empty() {
            let error = "Rich string must contain at least 1 (&Format, &str) tuple.";
            return Err(XlsxError::ParameterError(error.to_string()));
        }

        // Create a Style struct object to generate the font xml.
        let xf_formats: Vec<Format> = vec![];
        let mut styler = Styles::new(&xf_formats, 0, 0, 0, 0, false, true);
        let mut raw_string = "".to_string();

        let mut first_segment = true;
        for (format, string) in segments {
            // Excel doesn't allow empty string segments in a rich string.
            if string.is_empty() {
                let error = "Strings in rich string (&Format, &str) tuples cannot be blank.";
                return Err(XlsxError::ParameterError(error.to_string()));
            }

            // Accumulate the string segments into a unformatted string.
            raw_string.push_str(string);

            let attributes =
                if string.starts_with(['\t', '\n', ' ']) || string.ends_with(['\t', '\n', ' ']) {
                    vec![("xml:space", "preserve".to_string())]
                } else {
                    vec![]
                };

            // First segment doesn't require a font run for the default format.
            if format.is_default() && first_segment {
                styler.writer.xml_start_tag("r");
                styler
                    .writer
                    .xml_data_element_attr("t", string, &attributes);
                styler.writer.xml_end_tag("r");
            } else {
                styler.writer.xml_start_tag("r");
                styler.write_font(format);
                styler
                    .writer
                    .xml_data_element_attr("t", string, &attributes);
                styler.writer.xml_end_tag("r");
            }
            first_segment = false;
        }

        Ok((styler.writer.read_to_string(), raw_string))
    }

    // Insert a cell value into the worksheet data table structure.
    fn insert_cell(&mut self, row: RowNum, col: ColNum, cell: CellType) {
        match self.table.get_mut(&row) {
            Some(columns) => {
                // The row already exists. Insert/replace column value.
                columns.insert(col, cell);
            }
            None => {
                // The row doesn't exist, create a new row with columns and insert
                // the cell value.
                let mut columns: HashMap<ColNum, CellType> = HashMap::new();
                columns.insert(col, cell);
                self.table.insert(row, columns);
            }
        }
    }

    // Store the column width in Excel character units. Updates to the width can
    // come from the external user or from the internal autofit() routines.
    fn store_column_width(&mut self, col: ColNum, width: f64, autofit: bool) {
        // Excel has a maximum limit of 255 units for the column width.
        let mut width = width;
        if width > 255.0 {
            width = 255.0;
        }

        // Update an existing col metadata object or create a new one.
        match self.changed_cols.get_mut(&col) {
            Some(col_options) => {
                // Note, autofit() will only update a user defined value if is
                // greater than it. All other conditions are simple updates.
                if autofit && !col_options.autofit {
                    if width > col_options.width {
                        col_options.width = width;
                        col_options.autofit = true;
                    }
                } else {
                    col_options.width = width;
                    col_options.autofit = autofit;
                }
            }
            None => {
                // Create a new column metadata object.
                let col_options = ColOptions {
                    width,
                    xf_index: 0,
                    hidden: false,
                    autofit,
                };
                self.changed_cols.insert(col, col_options);
            }
        }
    }

    // Check that row and col are within the allowed Excel range and store max
    // and min values for use in other methods/elements.
    fn check_dimensions(&mut self, row: RowNum, col: ColNum) -> bool {
        // Check that the row an column number are within Excel's ranges.
        if row >= ROW_MAX {
            return false;
        }
        if col >= COL_MAX {
            return false;
        }

        // Store any changes in worksheet dimensions.
        self.dimensions.first_row = cmp::min(self.dimensions.first_row, row);
        self.dimensions.first_col = cmp::min(self.dimensions.first_col, col);
        self.dimensions.last_row = cmp::max(self.dimensions.last_row, row);
        self.dimensions.last_col = cmp::max(self.dimensions.last_col, col);

        true
    }

    // Check that row and col are within the allowed Excel range but don't
    // modify the worksheet cell range.
    fn check_dimensions_only(&mut self, row: RowNum, col: ColNum) -> bool {
        // Check that the row an column number are within Excel's ranges.
        if row >= ROW_MAX {
            return false;
        }
        if col >= COL_MAX {
            return false;
        }

        true
    }

    // Cached/faster version of utility.col_to_name() to use in the inner loop.
    fn col_to_name(&mut self, col_num: ColNum) -> String {
        if let Some(col_name) = self.col_names.get(&col_num) {
            col_name.clone()
        } else {
            let col_name = utility::col_to_name(col_num);
            self.col_names.insert(col_num, col_name.clone());
            col_name
        }
    }

    // Store local copies of unique formats passed to the write methods. These
    // indexes will be replaced by global/workbook indices before the worksheet
    // is saved.
    fn format_index(&mut self, format: &Format) -> u32 {
        let format_key = format.format_key();

        match self.xf_indices.get_mut(&format_key) {
            Some(xf_index) => *xf_index,
            None => {
                let xf_index = self.xf_formats.len() as u32;
                self.xf_formats.push(format.clone());
                self.xf_indices.insert(format_key, xf_index);
                if format.is_hyperlink {
                    self.has_hyperlink_style = true;
                }
                xf_index
            }
        }
    }

    // Set the mapping between the local format indices and the global/workbook
    // indices.
    pub(crate) fn set_global_xf_indices(&mut self, workbook_indices: &[u32]) {
        self.global_xf_indices = workbook_indices.to_owned();
    }

    // Translate the cell xf_index into a global/workbook format index. We also
    // need to make sure that an unformatted cell (xf_index == 0) takes the row
    // format (if it exists) or, failing that, the column format (if that
    // exists).
    fn get_cell_xf_index(
        &mut self,
        xf_index: &u32,
        row_options: Option<&RowOptions>,
        col_num: ColNum,
    ) -> u32 {
        // The local cell format index.
        let mut xf_index = *xf_index;

        // If it is zero the cell is unformatted and we check for a row format.
        if xf_index == 0 {
            if let Some(row_options) = row_options {
                xf_index = row_options.xf_index;
            }
        }

        // If it is still zero the row was unformatted so we check for a column
        // format.
        if xf_index == 0 {
            if let Some(col_options) = self.changed_cols.get(&col_num) {
                xf_index = col_options.xf_index;
            }
        }

        // Finally convert the local format index into a global/workbook index.
        if xf_index != 0 {
            xf_index = self.global_xf_indices[xf_index as usize];
        }

        xf_index
    }

    // Notes for the date/time handling functions below.
    //
    // * Datetimes in Excel are a serial date with days counted from an epoch
    //   (generally 1899-12-31) and the time as a percentage/decimal of the
    //   milliseconds in the day.
    //
    // * Both are stored in the same f64 value, for example, 2023/01/01 12:00:00 is
    //   stored as 44927.5 with a separate numeric format like yyyy/mm/dd hh:mm.
    //
    // * Excel can also save dates in a text ISO 8601 format in "Strict Open XML
    //   Spreadsheet" format but this is rarely used in practice.
    //
    // * Excel also doesn't use timezones or try to convert or encode timezone
    //   information in any way.

    // Convert a chrono::NaiveTime to an Excel serial datetime.
    fn datetime_to_excel(&mut self, datetime: &NaiveDateTime) -> f64 {
        let excel_date = self.date_to_excel(&datetime.date());
        let excel_time = self.time_to_excel(&datetime.time());

        excel_date + excel_time
    }

    // Convert a chrono::NaiveDate to an Excel serial date. In Excel a serial date
    // is the number of days since the epoch, which is either 1899-12-31 or
    // 1904-01-01.
    fn date_to_excel(&mut self, date: &NaiveDate) -> f64 {
        let epoch = NaiveDate::from_ymd_opt(1899, 12, 31).unwrap();

        let duration = *date - epoch;
        let mut excel_date = duration.num_days() as f64;

        // For legacy reasons Excel treats 1900 as a leap year. We add an additional
        // day for dates after the leapday in the 1899 epoch.
        if epoch.year() == 1899 && excel_date > 59.0 {
            excel_date += 1.0;
        }

        excel_date
    }

    // Convert a chrono::NaiveTime to an Excel time. The time portion of the Excel
    // datetime is the number of milliseconds divided by the total number of
    // milliseconds in the day.
    fn time_to_excel(&mut self, time: &NaiveTime) -> f64 {
        let midnight = NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap();
        let duration = *time - midnight;

        duration.num_milliseconds() as f64 / (24.0 * 60.0 * 60.0 * 1000.0)
    }

    // Convert the image dimensions into drawing dimensions and add them to the
    // Drawing object. Also set the rel linkages between the files.
    pub(crate) fn prepare_worksheet_images(
        &mut self,
        image_ids: &mut HashMap<u64, u32>,
        drawing_id: u32,
    ) {
        let mut rel_ids: HashMap<u64, u32> = HashMap::new();

        for (cell, image) in self.images.clone().iter() {
            let row = cell.0;
            let col = cell.1;

            let image_id = match image_ids.get(&image.hash) {
                Some(image_id) => *image_id,
                None => {
                    let image_id = 1 + image_ids.len() as u32;
                    image_ids.insert(image.hash, image_id);
                    image_id
                }
            };

            let rel_id = match rel_ids.get(&image.hash) {
                Some(rel_id) => *rel_id,
                None => {
                    let rel_id = 1 + rel_ids.len() as u32;
                    rel_ids.insert(image.hash, rel_id);

                    // Store the linkage to the drawings rels file.
                    let image_name =
                        format!("../media/image{image_id}.{}", image.image_type.extension());
                    self.drawing_relationships.push((
                        "image".to_string(),
                        image_name,
                        "".to_string(),
                    ));

                    rel_id
                }
            };

            // Convert the image dimensions to drawing dimensions and store the
            // drawing object.
            let mut drawing_info = self.position_object_emus(row, col, image);
            drawing_info.rel_id = rel_id;
            self.drawing.drawings.push(drawing_info);

            // Store the used image type for the Content Type file.
            self.image_types[image.image_type.clone() as usize] = true;
        }

        // Store the linkage to the worksheets rels file.
        let drawing_name = format!("../drawings/drawing{drawing_id}.xml");
        self.drawing_object_relationships.push((
            "drawing".to_string(),
            drawing_name,
            "".to_string(),
        ));

        self.has_drawing_object_linkage = true;
    }

    // Set up images used in headers and footers. Excel handles these
    // differently from worksheet images and stores them in a VML file rather
    // than an Drawing file.
    pub(crate) fn prepare_header_footer_images(
        &mut self,
        image_ids: &mut HashMap<u64, u32>,
        base_image_id: u32,
        drawing_id: u32,
    ) {
        let mut rel_ids: HashMap<u64, u32> = HashMap::new();
        for image in self.header_footer_images.clone().into_iter().flatten() {
            let image_id = match image_ids.get(&image.hash) {
                Some(image_id) => *image_id,
                None => {
                    let image_id = 1 + base_image_id + image_ids.len() as u32;
                    image_ids.insert(image.hash, image_id);
                    image_id
                }
            };

            let rel_id = match rel_ids.get(&image.hash) {
                Some(rel_id) => *rel_id,
                None => {
                    let rel_id = 1 + rel_ids.len() as u32;
                    rel_ids.insert(image.hash, rel_id);

                    // Store the linkage to the drawings rels file.
                    let image_name =
                        format!("../media/image{image_id}.{}", image.image_type.extension());
                    self.vml_drawing_relationships.push((
                        "image".to_string(),
                        image_name,
                        "".to_string(),
                    ));

                    rel_id
                }
            };

            // Header images are stored in a vmlDrawing file. We create a struct
            // to store the required image information in that format.
            let vml_info = VmlInfo {
                width: image.vml_width(),
                height: image.vml_height(),
                title: image.vml_name(),
                rel_id,
                position: image.vml_position(),
                is_scaled: image.is_scaled(),
            };

            // Store the header/footer vml data.
            self.header_footer_vml_info.push(vml_info);

            // Store the used image type for the Content Type file.
            self.image_types[image.image_type as usize] = true;
        }

        // Store the linkage to the worksheets rels file.
        let vml_drawing_name = format!("../drawings/vmlDrawing{drawing_id}.vml");
        self.drawing_object_relationships.push((
            "vmlDrawing".to_string(),
            vml_drawing_name,
            "".to_string(),
        ));
    }

    // Convert the chart dimensions into drawing dimensions and add them to the
    // Drawing object. Also set the rel linkages between the files.
    pub(crate) fn prepare_worksheet_charts(&mut self, mut chart_id: u32, drawing_id: u32) -> u32 {
        for (_, chart) in self.charts.iter_mut() {
            chart.id = chart_id;
            chart.add_axis_ids();
            chart_id += 1;
        }

        for (cell, chart) in self.charts.clone().iter_mut() {
            let row = cell.0;
            let col = cell.1;

            let chart_id = chart.id;

            // Store the linkage to the charts rels file.
            let chart_name = format!("../charts/chart{chart_id}.xml");
            self.drawing_relationships
                .push(("chart".to_string(), chart_name, "".to_string()));

            // Convert the chart dimensions to drawing dimensions and store the
            // drawing object.
            let drawing_info = self.position_object_emus(row, col, chart);
            self.drawing.drawings.push(drawing_info);
        }

        // Store the linkage to the worksheets rels file, if it hasn't already
        // been set by the one of the image preparation functions.
        if !self.has_drawing_object_linkage {
            let drawing_name = format!("../drawings/drawing{drawing_id}.xml");
            self.drawing_object_relationships.push((
                "drawing".to_string(),
                drawing_name,
                "".to_string(),
            ));
        }

        chart_id
    }

    // Calculate the vertices that define the position of a graphical object
    // within the worksheet in EMUs. The vertices are expressed as English
    // Metric Units (EMUs). There are 12,700 EMUs per point. Therefore, 12,700 *
    // 3 /4 = 9,525 EMUs per pixel.
    fn position_object_emus<T>(&mut self, row: RowNum, col: ColNum, object: &T) -> DrawingInfo
    where
        T: DrawingObject,
    {
        let mut drawing_info = self.position_object_pixels(row, col, object);

        // Convert the pixel values to EMUs.
        drawing_info.to.col_offset = round_to_emus(drawing_info.to.col_offset);
        drawing_info.to.row_offset = round_to_emus(drawing_info.to.row_offset);

        drawing_info.from.col_offset = round_to_emus(drawing_info.from.col_offset);
        drawing_info.from.row_offset = round_to_emus(drawing_info.from.row_offset);

        drawing_info.col_absolute *= 9525;
        drawing_info.row_absolute *= 9525;

        drawing_info.width = round_to_emus(drawing_info.width);
        drawing_info.height = round_to_emus(drawing_info.height);

        drawing_info
    }

    // Calculate the vertices that define the position of a graphical object
    // within the worksheet in pixels.
    //
    //         +------------+------------+
    //         |     A      |      B     |
    //   +-----+------------+------------+
    //   |     |(x1,y1)     |            |
    //   |  1  |(A1)._______|______      |
    //   |     |    |              |     |
    //   |     |    |              |     |
    //   +-----+----|    OBJECT    |-----+
    //   |     |    |              |     |
    //   |  2  |    |______________.     |
    //   |     |            |        (B2)|
    //   |     |            |     (x2,y2)|
    //   +---- +------------+------------+
    //
    // Example of an object that covers some of the area from cell A1 to  B2.
    //
    // Based on the width and height of the object we need to calculate 8 vars:
    //
    //     col_start, row_start, col_end, row_end, x1, y1, x2, y2.
    //
    // We also calculate the absolute x and y position of the top left vertex of
    // the object. This is required for images.
    //
    // The width and height of the cells that the object occupies can be
    // variable and have to be taken into account.
    //
    // The values of col_start and row_start are passed in from the calling
    // function. The values of col_end and row_end are calculated by subtracting
    // the width and height of the object from the width and height of the
    // underlying cells.
    //
    fn position_object_pixels<T>(&mut self, row: RowNum, col: ColNum, object: &T) -> DrawingInfo
    where
        T: DrawingObject,
    {
        let mut row_start: RowNum = row; // Row containing top left corner.
        let mut col_start: ColNum = col; // Column containing upper left corner.

        let mut x1: u32 = object.x_offset(); // Distance to left side of object.
        let mut y1: u32 = object.y_offset(); // Distance to top of object.

        let mut row_end: RowNum; // Row containing bottom right corner.
        let mut col_end: ColNum; // Column containing lower right corner.

        let mut x2: f64; // Distance to right side of object.
        let mut y2: f64; // Distance to bottom of object.

        let width = object.width_scaled(); // Width of object frame.
        let height = object.height_scaled(); // Height of object frame.

        let mut x_abs: u32 = 0; // Absolute distance to left side of object.
        let mut y_abs: u32 = 0; // Absolute distance to top  side of object.

        // Calculate the absolute x offset of the top-left vertex.
        for col in 0..col_start {
            x_abs += self.column_pixel_width(col, &object.object_movement());
        }
        x_abs += x1;

        // Calculate the absolute y offset of the top-left vertex.
        for row in 0..row_start {
            y_abs += self.row_pixel_height(row, &object.object_movement());
        }
        y_abs += y1;

        // Adjust start col for offsets that are greater than the col width.
        loop {
            let col_size = self.column_pixel_width(col_start, &object.object_movement());
            if x1 >= col_size {
                x1 -= col_size;
                col_start += 1;
            } else {
                break;
            }
        }

        // Adjust start row for offsets that are greater than the row height.
        loop {
            let row_size = self.row_pixel_height(row_start, &object.object_movement());
            if y1 >= row_size {
                y1 -= row_size;
                row_start += 1;
            } else {
                break;
            }
        }

        // Initialize end cell to the same as the start cell.
        col_end = col_start;
        row_end = row_start;

        // Calculate the end vertices.
        x2 = width + x1 as f64;
        y2 = height + y1 as f64;

        // Subtract the underlying cell widths to find the end cell.
        loop {
            let col_size = self.column_pixel_width(col_end, &object.object_movement()) as f64;
            if x2 >= col_size {
                x2 -= col_size;
                col_end += 1;
            } else {
                break;
            }
        }

        //Subtract the underlying cell heights to find the end cell.
        loop {
            let row_size = self.row_pixel_height(row_end, &object.object_movement()) as f64;
            if y2 >= row_size {
                y2 -= row_size;
                row_end += 1;
            } else {
                break;
            }
        }

        // Create structs to hold the drawing information.
        let from = DrawingCoordinates {
            col: col_start as u32,
            row: row_start,
            col_offset: x1 as f64,
            row_offset: y1 as f64,
        };

        let to = DrawingCoordinates {
            col: col_end as u32,
            row: row_end,
            col_offset: x2,
            row_offset: y2,
        };

        DrawingInfo {
            from,
            to,
            col_absolute: x_abs,
            row_absolute: y_abs,
            width,
            height,
            description: object.alt_text(),
            decorative: object.decorative(),
            object_movement: object.object_movement(),
            drawing_type: object.drawing_type(),
            rel_id: 0,
        }
    }

    // Convert the width of a cell from character units to pixels. Excel rounds
    // the column width to the nearest pixel.
    fn column_pixel_width(&mut self, col: ColNum, position: &ObjectMovement) -> u32 {
        let max_digit_width = 7.0_f64;
        let padding = 5.0_f64;

        match self.changed_cols.get(&col) {
            Some(col_options) => {
                let pixel_width = col_options.width;
                let hidden = col_options.hidden;

                if hidden && *position != ObjectMovement::MoveAndSizeWithCellsAfter {
                    // A hidden column is treated as having a width of zero unless
                    // the "object_movement" is MoveAndSizeWithCellsAfter.
                    0u32
                } else if pixel_width < 1.0 {
                    (pixel_width * (max_digit_width + padding) + 0.5) as u32
                } else {
                    (pixel_width * max_digit_width + 0.5) as u32 + padding as u32
                }
            }
            // If the width hasn't been set we use the default value.
            None => 64,
        }
    }

    // Convert the height of a cell from character units to pixels. If the
    // height hasn't been set by the user we use the default value.
    fn row_pixel_height(&mut self, row: RowNum, position: &ObjectMovement) -> u32 {
        match self.changed_rows.get(&row) {
            Some(row_options) => {
                let hidden = row_options.hidden;

                if hidden && *position != ObjectMovement::MoveAndSizeWithCellsAfter {
                    // A hidden row is treated as having a height of zero unless
                    // the "object_movement" is MoveAndSizeWithCellsAfter.
                    0u32
                } else {
                    (row_options.height * 4.0 / 3.0) as u32
                }
            }
            None => 20,
        }
    }

    // Reset an worksheet global data or structures between saves.
    pub(crate) fn reset(&mut self) {
        self.writer.reset();
        self.drawing.writer.reset();
        self.rel_count = 0;
        self.drawing.drawings.clear();
        self.hyperlink_relationships.clear();
        self.drawing_object_relationships.clear();
        self.drawing_relationships.clear();
        self.vml_drawing_relationships.clear();
        self.header_footer_vml_info.clear();
    }

    // Check if any external relationships are required.
    pub(crate) fn has_relationships(&self) -> bool {
        !self.hyperlink_relationships.is_empty() || !self.drawing_object_relationships.is_empty()
    }

    // Check if there is a header image.
    pub(crate) fn has_header_footer_images(&self) -> bool {
        self.header_footer_images[0].is_some()
            || self.header_footer_images[1].is_some()
            || self.header_footer_images[2].is_some()
            || self.header_footer_images[3].is_some()
            || self.header_footer_images[4].is_some()
            || self.header_footer_images[5].is_some()
    }

    // Check that there is a header/footer &[Picture] variable in the correct
    // position to match the corresponding image object.
    fn verify_header_footer_image(&self, string: &str, position: &HeaderImagePosition) -> bool {
        lazy_static! {
            static ref LEFT: Regex = Regex::new(r"(&[L].*)(:?&[CR])?").unwrap();
            static ref RIGHT: Regex = Regex::new(r"(&[R].*)(:?&[LC])?").unwrap();
            static ref CENTER: Regex = Regex::new(r"(&[C].*)(:?&[LR])?").unwrap();
        }

        let caps = match position {
            HeaderImagePosition::Left => LEFT.captures(string),
            HeaderImagePosition::Right => RIGHT.captures(string),
            HeaderImagePosition::Center => CENTER.captures(string),
        };

        match caps {
            Some(caps) => {
                let segment = caps.get(1).unwrap().as_str();
                segment.contains("&[Picture]") || segment.contains("&G")
            }
            None => false,
        }
    }

    // Convert column pixel width to character width.
    pub(crate) fn pixels_to_width(&mut self, pixels: u16) -> f64 {
        // Properties for Calibri 11.
        let max_digit_width = 7.0_f64;
        let padding = 5.0_f64;
        let mut width = pixels as f64;

        if width < 12.0 {
            width /= max_digit_width + padding;
        } else {
            width = (width - padding) / max_digit_width
        }

        width
    }

    // Return a range of data from a worksheet to use as cache data in a chart
    // file. If the range doesn't contain string/number data then we return a
    // default struct with an empty cache.
    pub(crate) fn get_cache_data(
        &self,
        first_row: RowNum,
        first_col: ColNum,
        last_row: RowNum,
        last_col: ColNum,
    ) -> ChartSeriesCacheData {
        let mut cache = ChartSeriesCacheData::new();
        let mut data = vec![];
        let mut is_numeric = true;

        for row_num in first_row..=last_row {
            match self.table.get(&row_num) {
                Some(columns) => {
                    for col_num in first_col..=last_col {
                        match columns.get(&col_num) {
                            Some(cell) => match cell {
                                CellType::String { string, .. } => {
                                    data.push(string.to_string());
                                    is_numeric = false;
                                }
                                CellType::Number { number, .. } => data.push(number.to_string()),
                                _ => return cache,
                            },
                            None => return cache,
                        }
                    }
                }
                None => return cache,
            }
        }

        cache.is_numeric = is_numeric;
        cache.data = data;
        cache
    }

    // -----------------------------------------------------------------------
    // XML assembly methods.
    // -----------------------------------------------------------------------

    //  Assemble and write the XML file.
    pub(crate) fn assemble_xml_file(&mut self, string_table: &mut SharedStringsTable) {
        self.writer.xml_declaration();

        // Write the worksheet element.
        self.write_worksheet();

        // Write the sheetPr element.
        self.write_sheet_pr();

        // Write the dimension element.
        self.write_dimension();

        // Write the sheetViews element.
        self.write_sheet_views();

        // Write the sheetFormatPr element.
        self.write_sheet_format_pr();

        // Write the cols element.
        self.write_cols();

        // Write the sheetData element.
        self.write_sheet_data(string_table);

        // Write the sheetProtection element.
        if self.protection_on {
            self.write_sheet_protection();
        }

        // Write the protectedRange element.
        if !self.unprotected_ranges.is_empty() {
            self.write_protected_ranges();
        }

        // Write the autoFilter element.
        if !self.autofilter_area.is_empty() {
            self.write_auto_filter();
        }

        // Write the mergeCells element.
        if !self.merged_ranges.is_empty() {
            self.write_merge_cells();
        }

        // Write the hyperlinks elements.
        if !self.hyperlinks.is_empty() {
            self.write_hyperlinks();
        }

        // Write the printOptions element.
        if self.print_options_changed {
            self.write_print_options();
        }

        // Write the pageMargins element.
        self.write_page_margins();

        // Write the pageSetup element.
        if self.page_setup_changed {
            self.write_page_setup();
        }

        // Write the headerFooter element.
        if self.head_footer_changed {
            self.write_header_footer();
        }

        // Write the rowBreaks element.
        if !self.horizontal_breaks.is_empty() {
            self.write_row_breaks();
        }

        // Write the colBreaks element.
        if !self.vertical_breaks.is_empty() {
            self.write_col_breaks();
        }

        // Write the drawing element.
        if !self.drawing.drawings.is_empty() {
            self.write_drawing();
        }

        // Write the legacyDrawingHF element.
        if self.has_header_footer_images() {
            self.write_legacy_drawing_hf();
        }

        // Close the worksheet tag.
        self.writer.xml_end_tag("worksheet");
    }

    // Write the <worksheet> element.
    fn write_worksheet(&mut self) {
        let xmlns = "http://schemas.openxmlformats.org/spreadsheetml/2006/main".to_string();
        let xmlns_r =
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships".to_string();

        let attributes = vec![("xmlns", xmlns), ("xmlns:r", xmlns_r)];

        self.writer.xml_start_tag_attr("worksheet", &attributes);
    }

    // Write the <sheetPr> element.
    fn write_sheet_pr(&mut self) {
        if self.filter_conditions.is_empty() && !self.fit_to_page && self.tab_color.is_default() {
            return;
        }

        let mut attributes = vec![];
        if !self.filter_conditions.is_empty() {
            attributes.push(("filterMode", "1".to_string()));
        }

        if self.fit_to_page || self.tab_color.is_not_default() {
            self.writer.xml_start_tag_attr("sheetPr", &attributes);

            // Write the pageSetUpPr element.
            self.write_page_set_up_pr();

            // Write the tabColor element.
            self.write_tab_color();

            self.writer.xml_end_tag("sheetPr");
        } else {
            self.writer.xml_empty_tag_attr("sheetPr", &attributes);
        }
    }

    // Write the <pageSetUpPr> element.
    fn write_page_set_up_pr(&mut self) {
        if !self.fit_to_page {
            return;
        }

        let attributes = vec![("fitToPage", "1".to_string())];

        self.writer.xml_empty_tag_attr("pageSetUpPr", &attributes);
    }

    // Write the <tabColor> element.
    fn write_tab_color(&mut self) {
        if self.tab_color.is_default() {
            return;
        }

        let attributes = self.tab_color.attributes();

        self.writer.xml_empty_tag_attr("tabColor", &attributes);
    }

    // Write the <dimension> element.
    fn write_dimension(&mut self) {
        let mut attributes = vec![];
        let mut range = "A1".to_string();

        if self.dimensions.first_row != ROW_MAX
            || self.dimensions.first_col != COL_MAX
            || self.dimensions.last_row != 0
            || self.dimensions.last_col != 0
        {
            range = utility::cell_range(
                self.dimensions.first_row,
                self.dimensions.first_col,
                self.dimensions.last_row,
                self.dimensions.last_col,
            );
        }

        attributes.push(("ref", range));

        self.writer.xml_empty_tag_attr("dimension", &attributes);
    }

    // Write the <sheetViews> element.
    fn write_sheet_views(&mut self) {
        self.writer.xml_start_tag("sheetViews");

        // Write the sheetView element.
        self.write_sheet_view();

        self.writer.xml_end_tag("sheetViews");
    }

    // Write the <sheetView> element.
    fn write_sheet_view(&mut self) {
        let mut attributes = vec![];

        if self.right_to_left {
            attributes.push(("rightToLeft", "1".to_string()));
        }

        if self.selected {
            attributes.push(("tabSelected", "1".to_string()));
        }

        match self.page_view {
            PageView::PageLayout => {
                attributes.push(("view", "pageLayout".to_string()));
            }
            PageView::PageBreaks => {
                attributes.push(("view", "pageBreakPreview".to_string()));
            }
            PageView::Normal => {}
        }

        if !self.top_left_cell.is_empty() {
            attributes.push(("topLeftCell", self.top_left_cell.clone()));
        }

        if self.zoom != 100 {
            attributes.push(("zoomScale", self.zoom.to_string()));

            match self.page_view {
                PageView::PageLayout => {
                    attributes.push(("zoomScalePageLayoutView", self.zoom.to_string()));
                }
                PageView::PageBreaks => {
                    attributes.push(("zoomScaleSheetLayoutView", self.zoom.to_string()));
                }
                PageView::Normal => {
                    attributes.push(("zoomScaleNormal", self.zoom.to_string()));
                }
            }
        }

        attributes.push(("workbookViewId", "0".to_string()));

        if self.panes.is_empty() && self.selected_range.0.is_empty() {
            self.writer.xml_empty_tag_attr("sheetView", &attributes);
        } else {
            self.writer.xml_start_tag_attr("sheetView", &attributes);
            self.write_panes();
            self.write_selections();
            self.writer.xml_end_tag("sheetView");
        }
    }

    // Write the elements associated with cell selections.
    fn write_selections(&mut self) {
        if self.selected_range.0.is_empty() {
            return;
        }

        let active_cell = self.selected_range.0.clone();
        let range = self.selected_range.1.clone();

        self.write_selection("", &active_cell, &range);
    }

    // Write the elements associated with panes.
    fn write_panes(&mut self) {
        if self.panes.is_empty() {
            return;
        }

        let row = self.panes.freeze_cell.0;
        let col = self.panes.freeze_cell.1;

        // Write the pane and selection elements.
        if row > 0 && col > 0 {
            self.write_pane("bottomRight");
            self.write_selection(
                "topRight",
                &utility::rowcol_to_cell(0, col),
                &utility::rowcol_to_cell(0, col),
            );
            self.write_selection(
                "bottomLeft",
                &utility::rowcol_to_cell(row, 0),
                &utility::rowcol_to_cell(row, 0),
            );
            self.write_selection("bottomRight", "", "");
        } else if col > 0 {
            self.write_pane("topRight");
            self.write_selection("topRight", "", "");
        } else {
            self.write_pane("bottomLeft");
            self.write_selection("bottomLeft", "", "");
        }
    }

    // Write the <pane> element.
    fn write_pane(&mut self, active_pane: &str) {
        let row = self.panes.freeze_cell.0;
        let col = self.panes.freeze_cell.1;
        let mut attributes = vec![];

        if col > 0 {
            attributes.push(("xSplit", col.to_string()));
        }

        if row > 0 {
            attributes.push(("ySplit", row.to_string()));
        }

        attributes.push(("topLeftCell", self.panes.top_left()));
        attributes.push(("activePane", active_pane.to_string()));
        attributes.push(("state", "frozen".to_string()));

        self.writer.xml_empty_tag_attr("pane", &attributes);
    }

    // Write the <selection> element.
    fn write_selection(&mut self, position: &str, active_cell: &str, range: &str) {
        let mut attributes = vec![];

        if !position.is_empty() {
            attributes.push(("pane", position.to_string()));
        }

        if !active_cell.is_empty() {
            attributes.push(("activeCell", active_cell.to_string()));
        }

        if !range.is_empty() {
            attributes.push(("sqref", range.to_string()));
        }

        self.writer.xml_empty_tag_attr("selection", &attributes);
    }

    // Write the <sheetFormatPr> element.
    fn write_sheet_format_pr(&mut self) {
        let attributes = vec![("defaultRowHeight", "15".to_string())];

        self.writer.xml_empty_tag_attr("sheetFormatPr", &attributes);
    }

    // Write the <sheetData> element.
    fn write_sheet_data(&mut self, string_table: &mut SharedStringsTable) {
        if self.table.is_empty() && self.changed_rows.is_empty() {
            self.writer.xml_empty_tag("sheetData");
        } else {
            self.writer.xml_start_tag("sheetData");
            self.write_data_table(string_table);
            self.writer.xml_end_tag("sheetData");
        }
    }

    // Write the <mergeCells> element.
    fn write_merge_cells(&mut self) {
        let attributes = vec![("count", self.merged_ranges.len().to_string())];

        self.writer.xml_start_tag_attr("mergeCells", &attributes);

        for merge_range in &self.merged_ranges.clone() {
            // Write the mergeCell element.
            self.write_merge_cell(merge_range);
        }

        self.writer.xml_end_tag("mergeCells");
    }

    // Write the <mergeCell> element.
    fn write_merge_cell(&mut self, merge_range: &CellRange) {
        let attributes = vec![("ref", merge_range.to_range_string())];

        self.writer.xml_empty_tag_attr("mergeCell", &attributes);
    }

    // Write the <hyperlinks> element.
    fn write_hyperlinks(&mut self) {
        self.writer.xml_start_tag("hyperlinks");

        let mut ref_id = 1u16;
        for (cell, hyperlink) in &mut self.hyperlinks.clone() {
            ref_id = hyperlink.increment_ref_id(ref_id);
            self.write_hyperlink(cell.0, cell.1, hyperlink);
        }

        self.rel_count = ref_id - 1;

        self.writer.xml_end_tag("hyperlinks");
    }

    // Write the <hyperlink> element.
    fn write_hyperlink(&mut self, row: RowNum, col: ColNum, hyperlink: &Hyperlink) {
        let mut attributes = vec![("ref", utility::rowcol_to_cell(row, col))];

        match hyperlink.link_type {
            HyperlinkType::Url | HyperlinkType::File => {
                let ref_id = hyperlink.ref_id;
                attributes.push(("r:id", format!("rId{ref_id}")));

                if !hyperlink.location.is_empty() {
                    attributes.push(("location", hyperlink.location.to_string()));
                }

                if !hyperlink.tip.is_empty() {
                    attributes.push(("tooltip", hyperlink.tip.to_string()));
                }

                // Store the linkage to the worksheets rels file.
                self.hyperlink_relationships.push((
                    "hyperlink".to_string(),
                    hyperlink.url.to_string(),
                    "External".to_string(),
                ));
            }
            HyperlinkType::Internal => {
                // Internal links don't use the rel file reference id.
                attributes.push(("location", hyperlink.location.to_string()));

                if !hyperlink.tip.is_empty() {
                    attributes.push(("tooltip", hyperlink.tip.to_string()));
                }

                attributes.push(("display", hyperlink.text.to_string()));
            }
            _ => {}
        }

        self.writer.xml_empty_tag_attr("hyperlink", &attributes);
    }

    // Write the <printOptions> element.
    fn write_print_options(&mut self) {
        let mut attributes = vec![];

        if self.center_horizontally {
            attributes.push(("horizontalCentered", "1".to_string()));
        }

        if self.center_vertically {
            attributes.push(("verticalCentered", "1".to_string()));
        }

        if self.print_headings {
            attributes.push(("headings", "1".to_string()));
        }

        if self.print_gridlines {
            attributes.push(("gridLines", "1".to_string()));
        }

        self.writer.xml_empty_tag_attr("printOptions", &attributes);
    }

    // Write the <pageMargins> element.
    fn write_page_margins(&mut self) {
        let attributes = vec![
            ("left", self.margin_left.to_string()),
            ("right", self.margin_right.to_string()),
            ("top", self.margin_top.to_string()),
            ("bottom", self.margin_bottom.to_string()),
            ("header", self.margin_header.to_string()),
            ("footer", self.margin_footer.to_string()),
        ];

        self.writer.xml_empty_tag_attr("pageMargins", &attributes);
    }

    // Write the <pageSetup> element.
    fn write_page_setup(&mut self) {
        let mut attributes = vec![];

        if self.paper_size > 0 {
            attributes.push(("paperSize", self.paper_size.to_string()));
        }

        if self.print_scale != 100 {
            attributes.push(("scale", self.print_scale.to_string()));
        }

        if self.fit_to_page {
            if self.fit_width != 1 {
                attributes.push(("fitToWidth", self.fit_width.to_string()));
            }
            if self.fit_height != 1 {
                attributes.push(("fitToHeight", self.fit_height.to_string()));
            }
        }

        if !self.default_page_order {
            attributes.push(("pageOrder", "overThenDown".to_string()));
        }

        if self.portrait {
            attributes.push(("orientation", "portrait".to_string()));
        } else {
            attributes.push(("orientation", "landscape".to_string()));
        }

        if self.first_page_number > 0 {
            attributes.push(("useFirstPageNumber", self.first_page_number.to_string()));
        }

        if self.print_black_and_white {
            attributes.push(("blackAndWhite", "1".to_string()));
        }

        if self.print_draft {
            attributes.push(("draft", "1".to_string()));
        }

        attributes.push(("horizontalDpi", "200".to_string()));
        attributes.push(("verticalDpi", "200".to_string()));

        self.writer.xml_empty_tag_attr("pageSetup", &attributes);
    }

    // Write the <autoFilter> element.
    fn write_auto_filter(&mut self) {
        let attributes = vec![("ref", self.autofilter_area.clone())];

        if self.filter_conditions.is_empty() {
            self.writer.xml_empty_tag_attr("autoFilter", &attributes);
        } else {
            self.writer.xml_start_tag_attr("autoFilter", &attributes);
            let col_offset = self.autofilter_defined_name.first_col;

            for col in self.filter_conditions.clone().keys().sorted() {
                let filter_condition = self.filter_conditions.get(col).unwrap().clone();

                self.write_filter_column(*col - col_offset, &filter_condition);
            }

            self.writer.xml_end_tag("autoFilter");
        }
    }

    // Write the <filterColumn> element.
    fn write_filter_column(&mut self, col: ColNum, filter_condition: &FilterCondition) {
        let attributes = vec![("colId", col.to_string())];

        self.writer.xml_start_tag_attr("filterColumn", &attributes);

        if filter_condition.is_list_filter {
            self.write_list_filters(filter_condition);
        } else {
            self.write_custom_filters(filter_condition)
        }

        self.writer.xml_end_tag("filterColumn");
    }

    // Write the <filters> element.
    fn write_list_filters(&mut self, filter_condition: &FilterCondition) {
        let mut attributes = vec![];

        if filter_condition.should_match_blanks {
            attributes.push(("blank", "1".to_string()));
        }

        if filter_condition.list.is_empty() {
            self.writer.xml_empty_tag_attr("filters", &attributes);
        } else {
            self.writer.xml_start_tag_attr("filters", &attributes);

            for data in &filter_condition.list {
                // Write the filter element.
                self.write_filter(data.string.clone());
            }

            self.writer.xml_end_tag("filters");
        }
    }

    // Write the <filter> element.
    fn write_filter(&mut self, value: String) {
        let attributes = vec![("val", value)];

        self.writer.xml_empty_tag_attr("filter", &attributes);
    }

    // Write the <customFilters> element.
    fn write_custom_filters(&mut self, filter_condition: &FilterCondition) {
        let mut attributes = vec![];

        if !filter_condition.apply_logical_or {
            attributes.push(("and", "1".to_string()));
        }

        self.writer.xml_start_tag_attr("customFilters", &attributes);

        if let Some(data) = filter_condition.custom1.as_ref() {
            self.write_custom_filter(data);
        }
        if let Some(data) = filter_condition.custom2.as_ref() {
            self.write_custom_filter(data);
        }

        self.writer.xml_end_tag("customFilters");
    }

    // Write the <customFilter> element.
    fn write_custom_filter(&mut self, data: &FilterData) {
        let mut attributes = vec![];

        if !data.criteria.operator().is_empty() {
            attributes.push(("operator", data.criteria.operator()));
        }

        attributes.push(("val", data.value()));

        self.writer.xml_empty_tag_attr("customFilter", &attributes);
    }

    // Write out all the row and cell data in the worksheet data table.
    fn write_data_table(&mut self, string_table: &mut SharedStringsTable) {
        let spans = self.calculate_spans();

        // Swap out the worksheet data structures so we can iterate over it and
        // still call self.write_xml() methods.
        let mut temp_table: HashMap<RowNum, HashMap<ColNum, CellType>> = HashMap::new();
        let mut temp_changed_rows: HashMap<RowNum, RowOptions> = HashMap::new();
        mem::swap(&mut temp_table, &mut self.table);
        mem::swap(&mut temp_changed_rows, &mut self.changed_rows);

        for row_num in self.dimensions.first_row..=self.dimensions.last_row {
            let span_index = row_num / 16;
            let span = spans.get(&span_index);

            let columns = temp_table.get(&row_num);
            let row_options = temp_changed_rows.get(&row_num);

            if columns.is_some() || row_options.is_some() {
                if let Some(columns) = columns {
                    self.write_row(row_num, span, row_options, true);
                    for col_num in self.dimensions.first_col..=self.dimensions.last_col {
                        if let Some(cell) = columns.get(&col_num) {
                            match cell {
                                CellType::Number { number, xf_index }
                                | CellType::DateTime { number, xf_index } => {
                                    let xf_index =
                                        self.get_cell_xf_index(xf_index, row_options, col_num);
                                    self.write_number_cell(row_num, col_num, number, &xf_index)
                                }
                                CellType::String { string, xf_index }
                                | CellType::RichString {
                                    string, xf_index, ..
                                } => {
                                    let xf_index =
                                        self.get_cell_xf_index(xf_index, row_options, col_num);
                                    let string_index = string_table.shared_string_index(string);
                                    self.write_string_cell(
                                        row_num,
                                        col_num,
                                        &string_index,
                                        &xf_index,
                                    );
                                }
                                CellType::Formula {
                                    formula,
                                    xf_index,
                                    result,
                                } => {
                                    let xf_index =
                                        self.get_cell_xf_index(xf_index, row_options, col_num);
                                    self.write_formula_cell(
                                        row_num, col_num, formula, &xf_index, result,
                                    )
                                }
                                CellType::ArrayFormula {
                                    formula,
                                    xf_index,
                                    result,
                                    is_dynamic,
                                    range,
                                } => {
                                    let xf_index =
                                        self.get_cell_xf_index(xf_index, row_options, col_num);
                                    self.write_array_formula_cell(
                                        row_num, col_num, formula, &xf_index, result, is_dynamic,
                                        range,
                                    )
                                }
                                CellType::Blank { xf_index } => {
                                    let xf_index =
                                        self.get_cell_xf_index(xf_index, row_options, col_num);
                                    self.write_blank_cell(row_num, col_num, &xf_index);
                                }
                                CellType::Boolean { boolean, xf_index } => {
                                    let xf_index =
                                        self.get_cell_xf_index(xf_index, row_options, col_num);
                                    self.write_boolean_cell(row_num, col_num, boolean, &xf_index);
                                }
                            }
                        }
                    }
                    self.writer.xml_end_tag("row");
                } else {
                    self.write_row(row_num, span, row_options, false);
                }
            }
        }

        // Swap back in data.
        mem::swap(&mut temp_table, &mut self.table);
        mem::swap(&mut temp_changed_rows, &mut self.changed_rows);
    }

    // Calculate the "spans" attribute of the <row> tag. This is an xlsx
    // optimization and isn't strictly required. However, it makes comparing
    // files easier. The span is the same for each block of 16 rows.
    fn calculate_spans(&mut self) -> HashMap<u32, String> {
        let mut spans: HashMap<RowNum, String> = HashMap::new();
        let mut span_min = COL_MAX;
        let mut span_max = 0;

        for row_num in self.dimensions.first_row..=self.dimensions.last_row {
            if let Some(columns) = self.table.get(&row_num) {
                for col_num in self.dimensions.first_col..=self.dimensions.last_col {
                    match columns.get(&col_num) {
                        Some(_) => {
                            if span_min == COL_MAX {
                                span_min = col_num;
                                span_max = col_num;
                            } else {
                                span_min = cmp::min(span_min, col_num);
                                span_max = cmp::max(span_max, col_num);
                            }
                        }
                        _ => continue,
                    }
                }
            }

            // Store the span range for each block or 16 rows.
            if (row_num + 1) % 16 == 0 || row_num == self.dimensions.last_row {
                let span_index = row_num / 16;
                if span_min != COL_MAX {
                    span_min += 1;
                    span_max += 1;
                    let span_range = format!("{span_min}:{span_max}");
                    spans.insert(span_index, span_range);
                    span_min = COL_MAX;
                }
            }
        }

        spans
    }

    // Write the <row> element.
    fn write_row(
        &mut self,
        row_num: RowNum,
        span: Option<&String>,
        row_options: Option<&RowOptions>,
        has_data: bool,
    ) {
        let row_num = format!("{}", row_num + 1);
        let mut attributes = vec![("r", row_num)];

        if let Some(span_range) = span {
            attributes.push(("spans", span_range.clone()));
        }

        if let Some(row_options) = row_options {
            let mut xf_index = row_options.xf_index;

            if xf_index != 0 {
                xf_index = self.global_xf_indices[xf_index as usize];
                attributes.push(("s", xf_index.to_string()));
                attributes.push(("customFormat", "1".to_string()));
            }

            if row_options.height != DEFAULT_ROW_HEIGHT {
                attributes.push(("ht", row_options.height.to_string()));
            }

            if row_options.hidden {
                attributes.push(("hidden", "1".to_string()));
            }

            if row_options.height != DEFAULT_ROW_HEIGHT {
                attributes.push(("customHeight", "1".to_string()));
            }
        }

        if has_data {
            self.writer.xml_start_tag_attr("row", &attributes);
        } else {
            self.writer.xml_empty_tag_attr("row", &attributes);
        }
    }

    // Write the <c> element for a number.
    fn write_number_cell(&mut self, row: RowNum, col: ColNum, number: &f64, xf_index: &u32) {
        let col_name = self.col_to_name(col);
        let mut style = String::from("");

        if *xf_index > 0 {
            style = format!(r#" s="{}""#, *xf_index);
        }

        write!(
            &mut self.writer.xmlfile,
            r#"<c r="{}{}"{}><v>{}</v></c>"#,
            col_name,
            row + 1,
            style,
            number
        )
        .expect("Couldn't write to file");
    }

    // Write the <c> element for a string.
    fn write_string_cell(&mut self, row: RowNum, col: ColNum, string_index: &u32, xf_index: &u32) {
        let col_name = self.col_to_name(col);
        let mut style = String::from("");

        if *xf_index > 0 {
            style = format!(r#" s="{}""#, *xf_index);
        }

        write!(
            &mut self.writer.xmlfile,
            r#"<c r="{}{}"{} t="s"><v>{}</v></c>"#,
            col_name,
            row + 1,
            style,
            string_index
        )
        .expect("Couldn't write to file");
    }

    // Write the <c> element for a formula.
    fn write_formula_cell(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
        xf_index: &u32,
        result: &str,
    ) {
        let col_name = self.col_to_name(col);
        let mut style = String::from("");
        let mut result_type = String::from("");

        if *xf_index > 0 {
            style = format!(r#" s="{}""#, *xf_index);
        }

        if result.parse::<f64>().is_err() {
            result_type = String::from(r#" t="str""#);
        }

        write!(
            &mut self.writer.xmlfile,
            r#"<c r="{}{}"{}{}><f>{}</f><v>{}</v></c>"#,
            col_name,
            row + 1,
            style,
            result_type,
            crate::xmlwriter::escape_data(formula),
            crate::xmlwriter::escape_data(result),
        )
        .expect("Couldn't write to file");
    }

    // Write the <c> element for an array formula.
    #[allow(clippy::too_many_arguments)]
    fn write_array_formula_cell(
        &mut self,
        row: RowNum,
        col: ColNum,
        formula: &str,
        xf_index: &u32,
        result: &str,
        is_dynamic: &bool,
        range: &str,
    ) {
        let col_name = self.col_to_name(col);
        let mut style = String::from("");
        let mut cm = String::from("");
        let mut result_type = String::from("");

        if *xf_index > 0 {
            style = format!(r#" s="{}""#, *xf_index);
        }

        if *is_dynamic {
            cm = String::from(r#" cm="1""#);
        }

        if result.parse::<f64>().is_err() {
            result_type = String::from(r#" t="str""#);
        }

        write!(
            &mut self.writer.xmlfile,
            r#"<c r="{}{}"{}{}{}><f t="array" ref="{}">{}</f><v>{}</v></c>"#,
            col_name,
            row + 1,
            style,
            cm,
            result_type,
            range,
            crate::xmlwriter::escape_data(formula),
            crate::xmlwriter::escape_data(result),
        )
        .expect("Couldn't write to file");
    }

    // Write the <c> element for a blank cell.
    fn write_blank_cell(&mut self, row: RowNum, col: ColNum, xf_index: &u32) {
        let col_name = self.col_to_name(col);

        // Write formatted blank cells and ignore unformatted blank cells (like
        // Excel does).
        if *xf_index > 0 {
            let style = format!(r#" s="{}""#, *xf_index);

            write!(
                &mut self.writer.xmlfile,
                r#"<c r="{}{}"{}/>"#,
                col_name,
                row + 1,
                style
            )
            .expect("Couldn't write to file");
        }
    }

    // Write the <c> element for a boolean cell.
    fn write_boolean_cell(&mut self, row: RowNum, col: ColNum, boolean: &bool, xf_index: &u32) {
        let col_name = self.col_to_name(col);
        let mut style = String::from("");
        let boolean = i32::from(*boolean);

        if *xf_index > 0 {
            style = format!(r#" s="{}""#, *xf_index);
        }

        write!(
            &mut self.writer.xmlfile,
            r#"<c r="{}{}"{} t="b"><v>{}</v></c>"#,
            col_name,
            row + 1,
            style,
            boolean
        )
        .expect("Couldn't write to file");
    }

    // Write the <cols> element.
    fn write_cols(&mut self) {
        if self.changed_cols.is_empty() {
            return;
        }

        self.writer.xml_start_tag("cols");

        // We need to write contiguous equivalent columns as a range with first
        // and last columns, so we convert the HashMap to a sorted vector and
        // iterate over that.
        let changed_cols = self.changed_cols.clone();
        let mut col_options: Vec<_> = changed_cols.iter().collect();
        col_options.sort_by_key(|x| x.0);

        // Remove the first (key, value) tuple in the vector and use it to set
        // the initial/previous properties.
        let first_col_options = col_options.remove(0);
        let mut first_col = first_col_options.0;
        let mut prev_col_options = first_col_options.1;
        let mut last_col = first_col;

        for (col_num, col_options) in col_options.iter() {
            // Check if the column number is contiguous with the previous column
            // and if the format is the same.
            if **col_num == *last_col + 1 && col_options == &prev_col_options {
                last_col = col_num;
            } else {
                // If not write out the current range of columns and start again.
                self.write_col(first_col, last_col, prev_col_options);
                first_col = *col_num;
                last_col = first_col;
                prev_col_options = *col_options;
            }
        }

        // We will exit the previous loop with one unhandled column range.
        self.write_col(first_col, last_col, prev_col_options);

        self.writer.xml_end_tag("cols");
    }

    // Write the <col> element.
    fn write_col(&mut self, first_col: &ColNum, last_col: &ColNum, col_options: &ColOptions) {
        let mut attributes = vec![];
        let first_col = *first_col + 1;
        let last_col = *last_col + 1;
        let mut width = col_options.width;
        let mut xf_index = col_options.xf_index;
        let has_custom_width = width != DEFAULT_COL_WIDTH;
        let hidden = col_options.hidden;

        // The default col width changes to 0 for hidden columns.
        if width == DEFAULT_COL_WIDTH && hidden {
            width = 0.0;
        }

        // Convert column width from user units to character width.
        if width > 0.0 {
            // Properties for Calibri 11.
            let max_digit_width = 7.0_f64;
            let padding = 5.0_f64;

            if width < 1.0 {
                width = ((width * (max_digit_width + padding)).round() / max_digit_width * 256.0)
                    .floor()
                    / 256.0;
            } else {
                width = (((width * max_digit_width).round() + padding) / max_digit_width * 256.0)
                    .floor()
                    / 256.0;
            }
        }

        attributes.push(("min", first_col.to_string()));
        attributes.push(("max", last_col.to_string()));
        attributes.push(("width", width.to_string()));

        if xf_index > 0 {
            xf_index = self.global_xf_indices[xf_index as usize];
            attributes.push(("style", xf_index.to_string()));
        }

        if col_options.autofit {
            attributes.push(("bestFit", "1".to_string()));
        }

        if hidden {
            attributes.push(("hidden", "1".to_string()));
        }

        if has_custom_width || hidden {
            attributes.push(("customWidth", "1".to_string()));
        }

        self.writer.xml_empty_tag_attr("col", &attributes);
    }

    // Write the <headerFooter> element.
    fn write_header_footer(&mut self) {
        let mut attributes = vec![];

        if !self.header_footer_scale_with_doc {
            attributes.push(("scaleWithDoc", "0".to_string()));
        }

        if !self.header_footer_align_with_page {
            attributes.push(("alignWithMargins", "0".to_string()));
        }

        if self.header.is_empty() && self.footer.is_empty() {
            self.writer.xml_empty_tag_attr("headerFooter", &attributes);
        } else {
            self.writer.xml_start_tag_attr("headerFooter", &attributes);

            // Write the oddHeader element.
            if !self.header.is_empty() {
                self.write_odd_header();
            }

            // Write the oddFooter element.
            if !self.footer.is_empty() {
                self.write_odd_footer();
            }

            self.writer.xml_end_tag("headerFooter");
        }
    }

    // Write the <oddHeader> element.
    fn write_odd_header(&mut self) {
        let header = self
            .header
            .replace("&[Tab]", "&A")
            .replace("&[Date]", "&D")
            .replace("&[File]", "&F")
            .replace("&[Page]", "&P")
            .replace("&[Path]", "&Z")
            .replace("&[Time]", "&T")
            .replace("&[Pages]", "&N")
            .replace("&[Picture]", "&G");

        self.writer.xml_data_element("oddHeader", &header);
    }

    // Write the <oddFooter> element.
    fn write_odd_footer(&mut self) {
        let footer = self
            .footer
            .replace("&[Tab]", "&A")
            .replace("&[Date]", "&D")
            .replace("&[File]", "&F")
            .replace("&[Page]", "&P")
            .replace("&[Path]", "&Z")
            .replace("&[Time]", "&T")
            .replace("&[Pages]", "&N")
            .replace("&[Picture]", "&G");

        self.writer.xml_data_element("oddFooter", &footer);
    }

    // Write the <drawing> element.
    fn write_drawing(&mut self) {
        self.rel_count += 1;
        let attributes = vec![("r:id", format!("rId{}", self.rel_count))];

        self.writer.xml_empty_tag_attr("drawing", &attributes);
    }

    // Write the <legacyDrawingHF> element.
    fn write_legacy_drawing_hf(&mut self) {
        self.rel_count += 1;
        let attributes = vec![("r:id", format!("rId{}", self.rel_count))];

        self.writer
            .xml_empty_tag_attr("legacyDrawingHF", &attributes);
    }

    // Write the <sheetProtection> element.
    fn write_sheet_protection(&mut self) {
        let mut attributes = vec![];

        if self.protection_hash != 0x0000 {
            attributes.push(("password", format!("{:04X}", self.protection_hash)));
        }

        attributes.push(("sheet", "1".to_string()));

        if !self.protection_options.edit_objects {
            attributes.push(("objects", "1".to_string()));
        }

        if !self.protection_options.edit_scenarios {
            attributes.push(("scenarios", "1".to_string()));
        }

        if self.protection_options.format_cells {
            attributes.push(("formatCells", "0".to_string()));
        }

        if self.protection_options.format_columns {
            attributes.push(("formatColumns", "0".to_string()));
        }

        if self.protection_options.format_rows {
            attributes.push(("formatRows", "0".to_string()));
        }

        if self.protection_options.insert_columns {
            attributes.push(("insertColumns", "0".to_string()));
        }

        if self.protection_options.insert_rows {
            attributes.push(("insertRows", "0".to_string()));
        }

        if self.protection_options.insert_links {
            attributes.push(("insertHyperlinks", "0".to_string()));
        }

        if self.protection_options.delete_columns {
            attributes.push(("deleteColumns", "0".to_string()));
        }

        if self.protection_options.delete_rows {
            attributes.push(("deleteRows", "0".to_string()));
        }

        if !self.protection_options.select_locked_cells {
            attributes.push(("selectLockedCells", "1".to_string()));
        }

        if self.protection_options.sort {
            attributes.push(("sort", "0".to_string()));
        }

        if self.protection_options.use_autofilter {
            attributes.push(("autoFilter", "0".to_string()));
        }

        if self.protection_options.use_pivot_tables {
            attributes.push(("pivotTables", "0".to_string()));
        }

        if !self.protection_options.select_unlocked_cells {
            attributes.push(("selectUnlockedCells", "1".to_string()));
        }

        self.writer
            .xml_empty_tag_attr("sheetProtection", &attributes);
    }

    // Write the <protectedRanges> element.
    fn write_protected_ranges(&mut self) {
        self.writer.xml_start_tag("protectedRanges");

        for (range, name, hash) in self.unprotected_ranges.clone() {
            self.write_protected_range(range, name, hash);
        }

        self.writer.xml_end_tag("protectedRanges");
    }

    // Write the <protectedRange> element.
    fn write_protected_range(&mut self, range: String, name: String, hash: u16) {
        let mut attributes = vec![];

        if hash > 0 {
            attributes.push(("password", format!("{hash:04X}")));
        }

        attributes.push(("sqref", range));
        attributes.push(("name", name));

        self.writer
            .xml_empty_tag_attr("protectedRange", &attributes);
    }

    // Write the <rowBreaks> element.
    fn write_row_breaks(&mut self) {
        let attributes = vec![
            ("count", self.horizontal_breaks.len().to_string()),
            ("manualBreakCount", self.horizontal_breaks.len().to_string()),
        ];

        self.writer.xml_start_tag_attr("rowBreaks", &attributes);

        for row_num in self.horizontal_breaks.clone() {
            // Write the brk element.
            self.write_row_brk(row_num);
        }

        self.writer.xml_end_tag("rowBreaks");
    }

    // Write the row <brk> element.
    fn write_row_brk(&mut self, row_num: u32) {
        let attributes = vec![
            ("id", row_num.to_string()),
            ("max", "16383".to_string()),
            ("man", "1".to_string()),
        ];

        self.writer.xml_empty_tag_attr("brk", &attributes);
    }

    // Write the <colBreaks> element.
    fn write_col_breaks(&mut self) {
        let attributes = vec![
            ("count", self.vertical_breaks.len().to_string()),
            ("manualBreakCount", self.vertical_breaks.len().to_string()),
        ];

        self.writer.xml_start_tag_attr("colBreaks", &attributes);

        for col_num in self.vertical_breaks.clone() {
            // Write the brk element.
            self.write_col_brk(col_num);
        }

        self.writer.xml_end_tag("colBreaks");
    }

    // Write the col <brk> element.
    fn write_col_brk(&mut self, col_num: u32) {
        let attributes = vec![
            ("id", col_num.to_string()),
            ("max", "1048575".to_string()),
            ("man", "1".to_string()),
        ];

        self.writer.xml_empty_tag_attr("brk", &attributes);
    }
}

// -----------------------------------------------------------------------
// Helper enums/structs/functions.
// -----------------------------------------------------------------------

// Round to the closest integer number of emu units.
fn round_to_emus(dimension: f64) -> f64 {
    ((0.5 + dimension * 9525.0) as u32) as f64
}

// Utility method to strip equal sign and array braces from a formula and
// also expand out future and dynamic array formulas.
fn prepare_formula(formula: &str, expand_future_functions: bool) -> String {
    let mut formula = formula.to_string();

    // Remove array formula braces and the leading = if they exist.
    if formula.starts_with('{') {
        formula.remove(0);
    }
    if formula.starts_with('=') {
        formula.remove(0);
    }
    if formula.ends_with('}') {
        formula.pop();
    }

    // Exit if formula is already expanded by the user.
    if formula.contains("_xlfn.") {
        return formula;
    }

    // Expand dynamic formulas.
    formula = escape_dynamic_formulas1(&formula).into();
    formula = escape_dynamic_formulas2(&formula).into();

    if expand_future_functions {
        formula = escape_future_functions(&formula).into();
    }

    formula
}

// Escape/expand the dynamic formula _xlfn functions.
fn escape_dynamic_formulas1(formula: &str) -> Cow<str> {
    lazy_static! {
        static ref XLFN: Regex = Regex::new(
            r"\b(ANCHORARRAY|LAMBDA|LET|RANDARRAY|SEQUENCE|SINGLE|SORTBY|UNIQUE|XLOOKUP|XMATCH)\("
        )
        .unwrap();
    }
    XLFN.replace_all(formula, "_xlfn.$1(")
}

// Escape/expand the dynamic formula _xlfn._xlws. functions.
fn escape_dynamic_formulas2(formula: &str) -> Cow<str> {
    lazy_static! {
        static ref XLWS: Regex = Regex::new(r"\b(FILTER|SORT)\(").unwrap();
    }
    XLWS.replace_all(formula, "_xlfn._xlws.$1(")
}

// Escape/expand future/_xlfn functions.
fn escape_future_functions(formula: &str) -> Cow<str> {
    lazy_static! {
        static ref FUTURE: Regex = Regex::new(
            r"\b(ACOTH|ACOT|AGGREGATE|ARABIC|BASE|BETA\.DIST|BETA\.INV|BINOM\.DIST\.RANGE|BINOM\.DIST|BINOM\.INV|BITAND|BITLSHIFT|BITOR|BITRSHIFT|BITXOR|CEILING\.MATH|CEILING\.PRECISE|CHISQ\.DIST\.RT|CHISQ\.DIST|CHISQ\.INV\.RT|CHISQ\.INV|CHISQ\.TEST|COMBINA|CONCAT|CONFIDENCE\.NORM|CONFIDENCE\.T|COTH|COT|COVARIANCE\.P|COVARIANCE\.S|CSCH|CSC|DAYS|DECIMAL|ERF\.PRECISE|ERFC\.PRECISE|EXPON\.DIST|F\.DIST\.RT|F\.DIST|F\.INV\.RT|F\.INV|F\.TEST|FILTERXML|FLOOR\.MATH|FLOOR\.PRECISE|FORECAST\.ETS\.CONFINT|FORECAST\.ETS\.SEASONALITY|FORECAST\.ETS\.STAT|FORECAST\.ETS|FORECAST\.LINEAR|FORMULATEXT|GAMMA\.DIST|GAMMA\.INV|GAMMALN\.PRECISE|GAMMA|GAUSS|HYPGEOM\.DIST|IFNA|IFS|IMCOSH|IMCOT|IMCSCH|IMCSC|IMSECH|IMSEC|IMSINH|IMTAN|ISFORMULA|ISOWEEKNUM|LOGNORM\.DIST|LOGNORM\.INV|MAXIFS|MINIFS|MODE\.MULT|MODE\.SNGL|MUNIT|NEGBINOM\.DIST|NORM\.DIST|NORM\.INV|NORM\.S\.DIST|NORM\.S\.INV|NUMBERVALUE|PDURATION|PERCENTILE\.EXC|PERCENTILE\.INC|PERCENTRANK\.EXC|PERCENTRANK\.INC|PERMUTATIONA|PHI|POISSON\.DIST|QUARTILE\.EXC|QUARTILE\.INC|QUERYSTRING|RANK\.AVG|RANK\.EQ|RRI|SECH|SEC|SHEETS|SHEET|SKEW\.P|STDEV\.P|STDEV\.S|SWITCH|T\.DIST\.2T|T\.DIST\.RT|T\.DIST|T\.INV\.2T|T\.INV|T\.TEST|TEXTJOIN|UNICHAR|UNICODE|VAR\.P|VAR\.S|WEBSERVICE|WEIBULL\.DIST|XOR|Z\.TEST)\("
        )
        .unwrap();
    }
    FUTURE.replace_all(formula, "_xlfn.$1(")
}

// Check of a dynamic function/formula.
fn is_dynamic_function(formula: &str) -> bool {
    lazy_static! {
        static ref DYNAMIC_FUNCTION: Regex = Regex::new(
            r"\b(ANCHORARRAY|FILTER|LAMBDA|LET|RANDARRAY|SEQUENCE|SINGLE|SORTBY|SORT|UNIQUE|XLOOKUP|XMATCH)\("
        )
        .unwrap();
    }
    DYNAMIC_FUNCTION.is_match(formula)
}

#[derive(Clone)]
struct CellRange {
    first_row: RowNum,
    first_col: ColNum,
    last_row: RowNum,
    last_col: ColNum,
}

impl CellRange {
    fn to_range_string(&self) -> String {
        utility::cell_range(self.first_row, self.first_col, self.last_row, self.last_col)
    }

    fn to_error_string(&self) -> String {
        format!(
            "({}, {}, {}, {}) / {}",
            self.first_row,
            self.first_col,
            self.last_row,
            self.last_col,
            utility::cell_range(self.first_row, self.first_col, self.last_row, self.last_col)
        )
    }
}

#[derive(Clone)]
struct RowOptions {
    height: f64,
    xf_index: u32,
    hidden: bool,
}

#[derive(Clone, PartialEq)]
struct ColOptions {
    width: f64,
    xf_index: u32,
    hidden: bool,
    autofit: bool,
}

#[derive(Clone)]
enum CellType {
    ArrayFormula {
        formula: String,
        xf_index: u32,
        result: String,
        is_dynamic: bool,
        range: String,
    },
    Blank {
        xf_index: u32,
    },
    Boolean {
        boolean: bool,
        xf_index: u32,
    },
    Formula {
        formula: String,
        xf_index: u32,
        result: String,
    },
    Number {
        number: f64,
        xf_index: u32,
    },
    DateTime {
        number: f64,
        xf_index: u32,
    },
    String {
        string: String,
        xf_index: u32,
    },
    RichString {
        string: String,
        xf_index: u32,
        raw_string: String,
    },
}

#[derive(Clone, Copy)]
enum PageView {
    Normal,
    PageLayout,
    PageBreaks,
}

#[derive(Clone)]
struct Panes {
    freeze_cell: (RowNum, ColNum),
    top_cell: (RowNum, ColNum),
}

impl Panes {
    fn is_empty(&self) -> bool {
        self.freeze_cell.0 == 0 && self.freeze_cell.1 == 0
    }

    fn top_left(&self) -> String {
        if self.top_cell.0 == 0 && self.top_cell.1 == 0 {
            utility::rowcol_to_cell(self.freeze_cell.0, self.freeze_cell.1)
        } else {
            utility::rowcol_to_cell(self.top_cell.0, self.top_cell.1)
        }
    }
}

#[derive(Clone)]
// Simple struct for handling different Excel hyperlinks types.
struct Hyperlink {
    url: String,
    text: String,
    tip: String,
    location: String,
    link_type: HyperlinkType,
    ref_id: u16,
}

impl Hyperlink {
    fn new(url: &str, text: &str, tip: &str) -> Result<Hyperlink, XlsxError> {
        let mut hyperlink = Hyperlink {
            url: url.to_string(),
            text: text.to_string(),
            tip: tip.to_string(),
            location: "".to_string(),
            link_type: HyperlinkType::Unknown,
            ref_id: 0,
        };

        Self::initialize(&mut hyperlink);

        // Check the hyperlink string lengths are within Excel's limits. The text
        // length is checked by write_string_with_format().
        if hyperlink.url.chars().count() > MAX_URL_LEN
            || hyperlink.location.chars().count() > MAX_URL_LEN
            || hyperlink.tip.chars().count() > MAX_PARAMETER_LEN
        {
            return Err(XlsxError::MaxUrlLengthExceeded);
        }

        Ok(hyperlink)
    }

    // This method handles a variety of different string processing that needs
    // to be done for links and targets associated with Excel hyperlinks.
    fn initialize(&mut self) {
        lazy_static! {
            static ref URL: Regex = Regex::new(r"^(ftp|http)s?://").unwrap();
            static ref URL_ESCAPE: Regex = Regex::new(r"%[0-9a-fA-F]{2}").unwrap();
            static ref REMOTE_FILE: Regex = Regex::new(r"^(\\\\|\w:)").unwrap();
        }

        if URL.is_match(&self.url) {
            // Handle web links like http://.
            self.link_type = HyperlinkType::Url;

            if self.text.is_empty() {
                self.text = self.url.clone();
            }

            // Split the url into url + #anchor if that exists.
            let parts: Vec<&str> = self.url.splitn(2, '#').collect();
            if parts.len() == 2 {
                self.location = parts[1].to_string();
                self.url = parts[0].to_string();
            }
        } else if self.url.starts_with("mailto:") {
            // Handle mail address links.
            self.link_type = HyperlinkType::Url;

            if self.text.is_empty() {
                self.text = self.url.replacen("mailto:", "", 1);
            }
        } else if self.url.starts_with("internal:") {
            // Handle links to cells within the workbook.
            self.link_type = HyperlinkType::Internal;
            self.location = self.url.replacen("internal:", "", 1);

            if self.text.is_empty() {
                self.text = self.location.clone();
            }
        } else if self.url.starts_with("file://") {
            // Handle links to other files or cells in other Excel files.
            self.link_type = HyperlinkType::File;
            let bare_link = self.url.replacen("file:///", "", 1);
            let bare_link = bare_link.replacen("file://", "", 1);

            // Links to local files aren't prefixed with file:///.
            if !REMOTE_FILE.is_match(&bare_link) {
                self.url = bare_link.clone();
            }

            if self.text.is_empty() {
                self.text = bare_link;
            }

            // Split the url into url + #anchor if that exists.
            let parts: Vec<&str> = self.url.splitn(2, '#').collect();
            if parts.len() == 2 {
                self.location = parts[1].to_string();
                self.url = parts[0].to_string();
            }
        }

        // Escape any url characters in the url string.
        if !URL_ESCAPE.is_match(&self.url) {
            self.url = crate::xmlwriter::escape_url(&self.url).into();
        }
    }

    // Increment the ref id
    fn increment_ref_id(&mut self, ref_id: u16) -> u16 {
        match self.link_type {
            HyperlinkType::Url | HyperlinkType::File => {
                self.ref_id = ref_id;
                ref_id + 1
            }
            _ => ref_id,
        }
    }
}

#[derive(Clone)]
enum HyperlinkType {
    Unknown,
    Url,
    Internal,
    File,
}

// Struct to hold and transform data for the various defined names variants:
// user defined names, autofilters, print titles and print areas.
#[derive(Clone)]
pub(crate) struct DefinedName {
    pub(crate) in_use: bool,
    pub(crate) name: String,
    pub(crate) sort_name: String,
    pub(crate) range: String,
    pub(crate) quoted_sheet_name: String,
    pub(crate) index: u16,
    pub(crate) name_type: DefinedNameType,
    pub(crate) first_row: RowNum,
    pub(crate) first_col: ColNum,
    pub(crate) last_row: RowNum,
    pub(crate) last_col: ColNum,
}

impl DefinedName {
    pub(crate) fn new() -> DefinedName {
        DefinedName {
            in_use: false,
            name: "".to_string(),
            sort_name: "".to_string(),
            range: "".to_string(),
            quoted_sheet_name: "".to_string(),
            index: 0,
            name_type: DefinedNameType::Global,
            first_row: ROW_MAX,
            first_col: COL_MAX,
            last_row: 0,
            last_col: 0,
        }
    }

    pub(crate) fn initialize(&mut self, sheet_name: &str) {
        self.quoted_sheet_name = sheet_name.to_string();
        self.set_range();
        self.set_sort_name();
    }

    // Get the version of the defined name required by the App.xml file. Global
    // and Autofilter variants return the empty string and are ignored.
    pub(crate) fn app_name(&self) -> String {
        match self.name_type {
            DefinedNameType::Local => format!("{}!{}", self.quoted_sheet_name, self.name),
            DefinedNameType::PrintArea => format!("{}!Print_Area", self.quoted_sheet_name),
            DefinedNameType::Autofilter => "".to_string(),
            DefinedNameType::PrintTitles => format!("{}!Print_Titles", self.quoted_sheet_name),
            DefinedNameType::Global => {
                if self.range.contains('!') {
                    self.name.clone()
                } else {
                    "".to_string()
                }
            }
        }
    }

    pub(crate) fn name(&self) -> String {
        match self.name_type {
            DefinedNameType::PrintArea => "_xlnm.Print_Area".to_string(),
            DefinedNameType::Autofilter => "_xlnm._FilterDatabase".to_string(),
            DefinedNameType::PrintTitles => "_xlnm.Print_Titles".to_string(),
            _ => self.name.clone(),
        }
    }

    pub(crate) fn unquoted_sheet_name(&self) -> String {
        if self.quoted_sheet_name.starts_with('\'') && self.quoted_sheet_name.ends_with('\'') {
            self.quoted_sheet_name[1..self.quoted_sheet_name.len() - 1].to_string()
        } else {
            self.quoted_sheet_name.clone()
        }
    }

    // The defined names are stored in a sorted order based on lowercase
    // and modified versions of the actual defined name.
    pub(crate) fn set_sort_name(&mut self) {
        let mut sort_name = match self.name_type {
            DefinedNameType::PrintArea => "Print_Area{}".to_string(),
            DefinedNameType::Autofilter => "_FilterDatabase{}".to_string(),
            DefinedNameType::PrintTitles => "Print_Titles".to_string(),
            _ => self.name.clone(),
        };

        sort_name = sort_name.replace('\'', "");
        self.sort_name = sort_name.to_lowercase();
    }

    pub(crate) fn set_range(&mut self) {
        match self.name_type {
            DefinedNameType::Autofilter | DefinedNameType::PrintArea => {
                let range;
                if self.first_col == 0 && self.last_col == COL_MAX - 1 {
                    // The print range is the entire column range, therefore we
                    // create a row only range.
                    range = format!("${}:${}", self.first_row + 1, self.last_row + 1);
                } else if self.first_row == 0 && self.last_row == ROW_MAX - 1 {
                    // The print range is the entire row range, therefore we
                    // create a column only range.
                    range = format!(
                        "${}:${}",
                        utility::col_to_name(self.first_col),
                        utility::col_to_name(self.last_col)
                    );
                } else {
                    // Otherwise handle it as a standard cell range.
                    range = utility::cell_range_abs(
                        self.first_row,
                        self.first_col,
                        self.last_row,
                        self.last_col,
                    );
                }

                self.range = format!("{}!{}", self.quoted_sheet_name, range);
            }
            DefinedNameType::PrintTitles => {
                let mut range = "".to_string();

                if self.first_col != COL_MAX || self.last_col != 0 {
                    // Repeat columns.
                    range = format!(
                        "{}!${}:${}",
                        self.quoted_sheet_name,
                        utility::col_to_name(self.first_col),
                        utility::col_to_name(self.last_col)
                    );
                }

                if self.first_row != ROW_MAX || self.last_row != 0 {
                    // Repeat rows.
                    let row_range = format!(
                        "{}!${}:${}",
                        self.quoted_sheet_name,
                        self.first_row + 1,
                        self.last_row + 1
                    );

                    if range.is_empty() {
                        // The range is rows only.
                        range = row_range;
                    } else {
                        // Excel stores combined repeat rows and columns as a
                        // comma separated list.
                        range = format!("{range},{row_range}");
                    }
                }

                self.range = range;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DefinedNameType {
    Autofilter,
    Global,
    Local,
    PrintArea,
    PrintTitles,
}

// -----------------------------------------------------------------------
// Tests are in the worksheet sub-directory.
// -----------------------------------------------------------------------
mod tests;
