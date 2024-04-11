#[derive(Clone)]
pub struct DataValidation {
    pub _type: Option<String>,
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

impl Default for DataValidation {
    fn default() -> Self {
        DataValidation {
            _type: None,
            allow_blank: None,
            show_drop_down: None,
            show_input_message: None,
            show_error_message: None,
            error_title: None,
            error: None,
            prompt_title: None,
            prompt: None,
            formula1: None,
            formula2: None,
            sqref: (String::from(""), String::from("")),
        }
    }
}

impl DataValidation {
    pub fn new() -> Self {
        DataValidation {
            ..Default::default()
        }
    }

    pub fn set_type(&mut self, value: &str) -> &mut Self {
        self._type = Some(value.to_string());
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
        if let Some(_type) = &self._type {
            attributes.push(("type", _type.clone()));
        }
        if let Some(allow_blank) = &self.allow_blank {
            attributes.push(("allowBlank", allow_blank.to_string()));
        }
        if let Some(show_drop_down) = &self.show_drop_down {
            attributes.push(("showDropDown", show_drop_down.to_string()));
        }
        if let Some(show_input_message) = &self.show_input_message {
            attributes.push(("showInputMessage", show_input_message.to_string()));
        }
        if let Some(show_error_message) = &self.show_error_message {
            attributes.push(("showErrorMessage", show_error_message.to_string()));
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

        attributes.push(("sqref", self.sqref.0.clone() + ":" + &self.sqref.1.clone()));

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
