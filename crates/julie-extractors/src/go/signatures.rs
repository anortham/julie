/// Function and method signature building utilities
impl super::GoExtractor {
    #[allow(dead_code)]
    pub(super) fn build_function_signature(
        &self,
        func_keyword: &str,
        name: &str,
        parameters: &[String],
        return_type: Option<&str>,
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = return_type.map_or(String::new(), |t| format!(" {}", t));

        if func_keyword.is_empty() {
            format!("{}{}{}", name, params, return_part)
        } else {
            format!("{} {}{}{}", func_keyword, name, params, return_part)
        }
    }

    pub(super) fn build_function_signature_with_generics(
        &self,
        func_keyword: &str,
        name: &str,
        type_params: &str,
        parameters: &[String],
        return_type: Option<&str>,
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = return_type.map_or(String::new(), |t| format!(" {}", t));

        if func_keyword.is_empty() {
            format!("{}{}{}{}", name, type_params, params, return_part)
        } else {
            format!(
                "{} {}{}{}{}",
                func_keyword, name, type_params, params, return_part
            )
        }
    }

    #[allow(dead_code)]
    pub(super) fn build_method_signature(
        &self,
        type_params: &str,
        parameters: &[String],
        return_type: Option<&str>,
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = return_type.map_or(String::new(), |t| format!(" {}", t));
        format!("{}{}{}", type_params, params, return_part)
    }

    pub(super) fn build_method_signature_with_return_types(
        &self,
        type_params: &str,
        parameters: &[String],
        return_types: &[String],
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = match return_types.len() {
            0 => String::new(),
            1 => format!(" {}", return_types[0]),
            _ => format!(" ({})", return_types.join(", ")),
        };

        format!("{}{}{}", type_params, params, return_part)
    }

    pub(super) fn build_function_signature_with_return_types(
        &self,
        func_keyword: &str,
        name: &str,
        parameters: &[String],
        return_types: &[String],
    ) -> String {
        let params = if parameters.is_empty() {
            "()".to_string()
        } else {
            format!("({})", parameters.join(", "))
        };

        let return_part = match return_types.len() {
            0 => String::new(),
            1 => format!(" {}", return_types[0]),
            _ => format!(" ({})", return_types.join(", ")),
        };

        if func_keyword.is_empty() {
            format!("{}{}{}", name, params, return_part)
        } else {
            format!("{} {}{}{}", func_keyword, name, params, return_part)
        }
    }
}
