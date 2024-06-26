#[derive(Clone, Default)]
pub struct DataValidation {
    pub validation_type: Option<String>,
    // pub error_style: String
    // pub ime_mode: String
    // pub operator: String
    pub allow_blank: Option<bool>,
    pub show_drop_down: Option<bool>,
    pub show_input_message: Option<bool>,
    pub show_error_message: Option<bool>,
    pub error_title: Option<String>,
    pub error: Option<String>,
    pub prompt_title: Option<String>,
    pub prompt: Option<String>,
    pub formula1: Option<Formula1>,
    pub formula2: Option<Formula2>,
    pub sqref: (String, String),
}

impl DataValidation {
    pub fn new() -> Self {
        DataValidation {
            ..Default::default()
        }
    }

    pub fn set_type(&mut self, value: &str) -> &mut Self {
        self.validation_type = Some(value.to_string());
        self
    }

    pub fn set_allow_blank(&mut self, value: bool) -> &mut Self {
        self.allow_blank = Some(value);
        self
    }

    pub fn set_show_drop_down(&mut self, value: bool) -> &mut Self {
        self.show_drop_down = Some(value);
        self
    }

    pub fn set_show_input_message(&mut self, value: bool) -> &mut Self {
        self.show_input_message = Some(value);
        self
    }

    pub fn set_show_error_message(&mut self, value: bool) -> &mut Self {
        self.show_error_message = Some(value);
        self
    }

    pub fn set_error_title(&mut self, value: &str) -> &mut Self {
        self.error_title = Some(value.to_string());
        self
    }

    pub fn set_error(&mut self, value: &str) -> &mut Self {
        self.error = Some(value.to_string());
        self
    }

    pub fn set_prompt_title(&mut self, value: &str) -> &mut Self {
        self.prompt_title = Some(value.to_string());
        self
    }

    pub fn set_prompt(&mut self, value: &str) -> &mut Self {
        self.prompt = Some(value.to_string());
        self
    }

    pub fn set_formula1(&mut self, value: &str) -> &mut Self {
        self.formula1 = Some(Formula1 {
            value: value.to_string(),
        });
        self
    }

    pub fn set_formula2(&mut self, value: &str) -> &mut Self {
        self.formula2 = Some(Formula2 {
            value: value.to_string(),
        });
        self
    }

    pub fn set_sqref(&mut self, start: &str, end: &str) -> &mut Self {
        self.sqref = (start.to_string(), end.to_string());
        self
    }

    pub fn get_attributes(&self) -> Vec<(&str, String)> {
        let mut attributes = Vec::new();
        if let Some(validation_type) = &self.validation_type {
            attributes.push(("type", validation_type.clone()));
        }
        if let Some(allow_blank) = &self.allow_blank {
            attributes.push((
                "allowBlank",
                if *allow_blank { "1" } else { "0" }.to_string(),
            ));
        }
        if let Some(show_drop_down) = &self.show_drop_down {
            attributes.push((
                "showDropDown",
                if *show_drop_down { "1" } else { "0" }.to_string(),
            ));
        }
        if let Some(show_input_message) = &self.show_input_message {
            attributes.push((
                "showInputMessage",
                if *show_input_message { "1" } else { "0" }.to_string(),
            ));
        }
        if let Some(show_error_message) = &self.show_error_message {
            attributes.push((
                "showErrorMessage",
                if *show_error_message { "1" } else { "0" }.to_string(),
            ));
        }
        if let Some(error_title) = &self.error_title {
            attributes.push(("errorTitle", error_title.clone()));
        }
        if let Some(error) = &self.error {
            attributes.push(("error", error.clone()));
        }
        if let Some(prompt_title) = &self.prompt_title {
            attributes.push(("promptTitle", prompt_title.clone()));
        }
        if let Some(prompt) = &self.prompt {
            attributes.push(("prompt", prompt.clone()));
        }

        attributes.push(("sqref", format!("{}:{}", self.sqref.0, self.sqref.1)));

        attributes
    }
}

#[derive(Clone)]
pub struct Formula1 {
    pub value: String,
}

#[derive(Clone)]
pub struct Formula2 {
    pub value: String,
}
