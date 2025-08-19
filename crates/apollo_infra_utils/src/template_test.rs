use std::fmt::Display;

use pretty_assertions::assert_eq;

use crate::template::Template;

#[test]
fn templates() {
    // Test with a simple template and two arguments
    let template = Template::new("Hello, {}! Welcome to {}.");
    let args: Vec<&dyn Display> = vec![&"Alice", &"Wonderland"];
    let formatted = template.format(&args);
    assert_eq!(formatted, "Hello, Alice! Welcome to Wonderland.");

    // Test with a simple template and two arguments
    let template = Template::new("My two favorite numbers are {} and {}.");
    let args: Vec<&dyn Display> = vec![&1913, &1312];
    let formatted = template.format(&args);
    assert_eq!(formatted, "My two favorite numbers are 1913 and 1312.");

    // Test with an empty template
    let empty_template = Template::new("");
    let empty_args: Vec<&dyn Display> = vec![];
    let empty_formatted = empty_template.format(&empty_args);
    assert_eq!(empty_formatted, "");

    // Test with a template that is a single placeholder
    let placeholder_template = Template::new("{}");
    let placeholder_template_args: Vec<&dyn Display> = vec![&"MHFC"];
    let placeholder_template_formatted = placeholder_template.format(&placeholder_template_args);
    assert_eq!(placeholder_template_formatted, "MHFC");
}

#[test]
#[should_panic]
fn template_too_many_args() {
    let template = Template::new("{}");
    let args: Vec<&dyn Display> = vec![&"1", &"2"];
    template.format(&args);
}

#[test]
#[should_panic]
fn template_too_few_args() {
    let template = Template::new("{}{}{}");
    let args: Vec<&dyn Display> = vec![&"1", &"2"];
    template.format(&args);
}
