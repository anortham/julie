use super::{SymbolKind, extract_symbols};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_complex_forms_validation_and_interactive_elements() {
        let html_code = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Advanced Form Example</title>
</head>
<body>
  <section class="form-section">
    <h2>Contact Information</h2>

    <form id="contact-form" class="contact-form" novalidate aria-label="Contact form">
      <div class="form-row">
        <div class="form-group">
          <label for="first-name" class="form-label required">
            First Name
            <span class="required-indicator" aria-label="required">*</span>
          </label>
          <input
            type="text"
            id="first-name"
            name="firstName"
            class="form-input"
            required
            autocomplete="given-name"
            aria-describedby="first-name-error"
            minlength="2"
            maxlength="50"
            pattern="[A-Za-z\\s]+"
            placeholder="Enter your first name"
          >
          <div id="first-name-error" class="error-message" role="alert" aria-live="polite"></div>
        </div>

        <div class="form-group">
          <label for="last-name" class="form-label required">
            Last Name
            <span class="required-indicator" aria-label="required">*</span>
          </label>
          <input
            type="text"
            id="last-name"
            name="lastName"
            class="form-input"
            required
            autocomplete="family-name"
            aria-describedby="last-name-error"
            minlength="2"
            maxlength="50"
            pattern="[A-Za-z\\s]+"
            placeholder="Enter your last name"
          >
          <div id="last-name-error" class="error-message" role="alert" aria-live="polite"></div>
        </div>
      </div>

      <div class="form-group">
        <label for="email" class="form-label required">
          Email Address
          <span class="required-indicator" aria-label="required">*</span>
        </label>
        <input
          type="email"
          id="email"
          name="email"
          class="form-input"
          required
          autocomplete="email"
          aria-describedby="email-help email-error"
          placeholder="your.email@example.com"
        >
        <div id="email-help" class="form-help">
          We will never share your email with anyone else.
        </div>
        <div id="email-error" class="error-message" role="alert" aria-live="polite"></div>
      </div>

      <div class="form-group">
        <fieldset class="form-fieldset">
          <legend class="form-legend required">
            Preferred Contact Method
            <span class="required-indicator" aria-label="required">*</span>
          </legend>
          <div class="radio-group" role="radiogroup" aria-describedby="contact-method-error">
            <label class="radio-label">
              <input type="radio" name="contactMethod" value="email" class="radio-input" required>
              <span class="radio-text">Email</span>
            </label>
            <label class="radio-label">
              <input type="radio" name="contactMethod" value="phone" class="radio-input" required>
              <span class="radio-text">Phone</span>
            </label>
            <label class="radio-label">
              <input type="radio" name="contactMethod" value="both" class="radio-input" required>
              <span class="radio-text">Both Email and Phone</span>
            </label>
          </div>
          <div id="contact-method-error" class="error-message" role="alert" aria-live="polite"></div>
        </fieldset>
      </div>

      <div class="form-group">
        <fieldset class="form-fieldset">
          <legend class="form-legend">Interests</legend>
          <div class="checkbox-group" aria-describedby="interests-help">
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="web-development" class="checkbox-input">
              <span class="checkbox-text">Web Development</span>
            </label>
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="mobile-apps" class="checkbox-input">
              <span class="checkbox-text">Mobile Apps</span>
            </label>
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="ui-design" class="checkbox-input">
              <span class="checkbox-text">UI/UX Design</span>
            </label>
            <label class="checkbox-label">
              <input type="checkbox" name="interests" value="data-science" class="checkbox-input">
              <span class="checkbox-text">Data Science</span>
            </label>
          </div>
          <div id="interests-help" class="form-help">
            Select all that apply to your interests.
          </div>
        </fieldset>
      </div>

      <div class="form-actions">
        <button type="button" class="btn btn-secondary" data-action="save-draft">
          Save as Draft
        </button>
        <button type="reset" class="btn btn-ghost">
          Clear Form
        </button>
        <button type="submit" class="btn btn-primary">
          <span class="btn-text">Send Message</span>
          <span class="btn-loading" aria-hidden="true">Sending...</span>
        </button>
      </div>
    </form>
  </section>

  <dialog id="confirmation-modal" class="modal" aria-labelledby="modal-title" aria-describedby="modal-description">
    <div class="modal-content">
      <header class="modal-header">
        <h3 id="modal-title" class="modal-title">Confirm Submission</h3>
        <button type="button" class="modal-close" aria-label="Close dialog" data-action="close-modal">
          <span aria-hidden="true">&times;</span>
        </button>
      </header>

      <div class="modal-body">
        <p id="modal-description">
          Are you sure you want to submit this form? Please review your information before proceeding.
        </p>
      </div>

      <footer class="modal-footer">
        <button type="button" class="btn btn-secondary" data-action="cancel">
          Cancel
        </button>
        <button type="button" class="btn btn-primary" data-action="confirm-submit">
          Confirm & Submit
        </button>
      </footer>
    </div>
  </dialog>

  <details class="disclosure" open>
    <summary class="disclosure-summary">
      <span class="summary-text">Advanced Options</span>
      <span class="summary-icon" aria-hidden="true">▼</span>
    </summary>

    <div class="disclosure-content">
      <div class="form-group">
        <label for="timezone" class="form-label">Timezone</label>
        <select id="timezone" name="timezone" class="form-select">
          <option value="">Auto-detect</option>
          <option value="UTC">UTC</option>
          <option value="EST">Eastern Standard Time</option>
          <option value="PST">Pacific Standard Time</option>
          <option value="GMT">Greenwich Mean Time</option>
        </select>
      </div>
    </div>
  </details>
</body>
</html>"###;
        let symbols = extract_symbols(html_code);

        // Form element
        let contact_form = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"id="contact-form""#))
        });
        assert!(contact_form.is_some());
        assert!(
            contact_form
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("novalidate")
        );
        assert!(
            contact_form
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"aria-label="Contact form""#)
        );

        // Input elements with validation
        let first_name_input = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"id="first-name""#))
        });
        assert!(first_name_input.is_some());
        assert_eq!(first_name_input.unwrap().kind, SymbolKind::Field);
        assert!(
            first_name_input
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("required")
        );
        assert!(
            first_name_input
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"autocomplete="given-name""#)
        );
        assert!(
            first_name_input
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"pattern="[A-Za-z\\s]+""#)
        );

        let email_input = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"type="email""#))
        });
        assert!(email_input.is_some());
        assert!(
            email_input
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"autocomplete="email""#)
        );

        // Radio buttons
        let radio_inputs: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains(r#"type="radio""#))
            })
            .collect();
        assert_eq!(radio_inputs.len(), 3);

        let email_radio = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"name="contactMethod""#) && sig.contains(r#"value="email""#)
            })
        });
        assert!(email_radio.is_some());

        // Checkboxes
        let checkbox_inputs: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains(r#"type="checkbox""#))
            })
            .collect();
        assert!(checkbox_inputs.len() >= 4);

        let web_dev_checkbox = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"value="web-development""#))
        });
        assert!(web_dev_checkbox.is_some());

        // Modal dialog
        let dialog_element = symbols.iter().find(|s| s.name == "dialog");
        assert!(dialog_element.is_some());
        assert!(
            dialog_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"aria-labelledby="modal-title""#)
        );
        assert!(
            dialog_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(r#"aria-describedby="modal-description""#)
        );

        // Details/Summary
        let details_element = symbols.iter().find(|s| s.name == "details");
        assert!(details_element.is_some());
        assert!(
            details_element
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("open")
        );

        let summary_element = symbols.iter().find(|s| s.name == "summary");
        assert!(summary_element.is_some());
    }
}
