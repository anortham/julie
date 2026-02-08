// Dart Extractor Tests
//
// Direct Implementation of Dart extractor tests (TDD RED phase)

// Submodule declarations
pub mod extractor;
pub mod cross_file_relationships;

use crate::base::SymbolKind;
use crate::dart::DartExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Initialize Dart parser for Dart files
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&harper_tree_sitter_dart::LANGUAGE.into())
        .expect("Error loading Dart grammar");
    parser
}

#[cfg(test)]
mod dart_extractor_tests {
    use super::*;

    mod classes_and_constructors {
        use super::*;

        #[test]
        fn test_extract_classes_with_various_constructor_types() {
            let code = r#"
class Person {
  String name;
  int age;
  String? email;
  late bool isVerified;

  // Default constructor
  Person(this.name, this.age, {this.email});

  // Named constructor
  Person.baby(this.name) : age = 0;

  // Factory constructor
  factory Person.fromJson(Map<String, dynamic> json) {
    return Person(json['name'], json['age'], email: json['email']);
  }

  // Const constructor
  const Person.unknown() : name = 'Unknown', age = 0, email = null;

  void greet() {
    print('Hello, I am $name');
  }

  int get birthYear => DateTime.now().year - age;

  set newAge(int value) {
    age = value;
  }
}

abstract class Animal {
  String get sound;
  void makeSound() => print(sound);
}

class Dog extends Animal {
  @override
  String get sound => 'Woof!';

  static int totalDogs = 0;

  Dog() {
    totalDogs++;
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract classes
            let person_class = symbols
                .iter()
                .find(|s| s.name == "Person" && s.kind == SymbolKind::Class);
            assert!(person_class.is_some());
            let person_class = person_class.unwrap();
            assert!(
                person_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("class Person")
            );

            let animal_class = symbols
                .iter()
                .find(|s| s.name == "Animal" && s.kind == SymbolKind::Class);
            assert!(animal_class.is_some());
            let animal_class = animal_class.unwrap();
            assert!(
                animal_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("abstract class Animal")
            );

            let dog_class = symbols
                .iter()
                .find(|s| s.name == "Dog" && s.kind == SymbolKind::Class);
            assert!(dog_class.is_some());

            // Should extract constructors
            let constructors: Vec<_> = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Constructor)
                .collect();
            assert!(constructors.len() >= 4); // Default, named, factory, const

            let default_constructor = constructors.iter().find(|s| s.name == "Person");
            assert!(default_constructor.is_some());

            let named_constructor = constructors.iter().find(|s| s.name == "Person.baby");
            assert!(named_constructor.is_some());

            let factory_constructor = constructors.iter().find(|s| s.name == "Person.fromJson");
            assert!(factory_constructor.is_some());
            let factory_constructor = factory_constructor.unwrap();
            assert!(
                factory_constructor
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("factory")
            );

            // Should extract methods
            let greet_method = symbols.iter().find(|s| s.name == "greet");
            assert!(greet_method.is_some());
            let greet_method = greet_method.unwrap();
            assert_eq!(greet_method.kind, SymbolKind::Method);

            let make_sound_method = symbols.iter().find(|s| s.name == "makeSound");
            assert!(make_sound_method.is_some());

            // Should extract getters and setters
            let birth_year_getter = symbols.iter().find(|s| s.name == "birthYear");
            assert!(birth_year_getter.is_some());
            let birth_year_getter = birth_year_getter.unwrap();
            assert!(
                birth_year_getter
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("get")
            );

            let new_age_setter = symbols.iter().find(|s| s.name == "newAge");
            assert!(new_age_setter.is_some());
            let new_age_setter = new_age_setter.unwrap();
            assert!(new_age_setter.signature.as_ref().unwrap().contains("set"));

            // Should extract fields/properties
            let name_field = symbols.iter().find(|s| s.name == "name");
            assert!(name_field.is_some());
            let name_field = name_field.unwrap();
            assert_eq!(name_field.kind, SymbolKind::Field);

            let total_dogs_field = symbols.iter().find(|s| s.name == "totalDogs");
            assert!(total_dogs_field.is_some());
            let total_dogs_field = total_dogs_field.unwrap();
            assert!(
                total_dogs_field
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("static")
            );
        }
    }

    mod mixins_and_extensions {
        use super::*;

        #[test]
        fn test_extract_mixins_and_extensions() {
            let code = r#"
mixin Flyable {
  double altitude = 0;

  void fly() {
    altitude += 100;
    print('Flying at altitude $altitude');
  }

  void land() => altitude = 0;
}

mixin Swimmable on Animal {
  void swim() => print('Swimming like a ${sound.toLowerCase()}');
}

class Bird extends Animal with Flyable {
  @override
  String get sound => 'Tweet!';
}

class Duck extends Animal with Flyable, Swimmable {
  @override
  String get sound => 'Quack!';
}

extension StringExtensions on String {
  String get capitalized =>
    this.isNotEmpty ? '${this[0].toUpperCase()}${this.substring(1)}' : this;

  bool get isEmail => contains('@') && contains('.');

  String reverse() => split('').reversed.join('');
}

extension on List<int> {
  int get sum => fold(0, (a, b) => a + b);
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract mixins
            let flyable_mixin = symbols.iter().find(|s| s.name == "Flyable");
            assert!(flyable_mixin.is_some());
            let flyable_mixin = flyable_mixin.unwrap();
            assert!(
                flyable_mixin
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("mixin Flyable")
            );

            let swimmable_mixin = symbols.iter().find(|s| s.name == "Swimmable");
            assert!(swimmable_mixin.is_some());
            let swimmable_mixin = swimmable_mixin.unwrap();
            assert!(
                swimmable_mixin
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("mixin Swimmable on Animal")
            );

            // Should extract mixin methods
            let fly_method = symbols.iter().find(|s| s.name == "fly");
            assert!(fly_method.is_some());

            let swim_method = symbols.iter().find(|s| s.name == "swim");
            assert!(swim_method.is_some());

            // Should extract classes with mixins
            let bird_class = symbols.iter().find(|s| s.name == "Bird");
            assert!(bird_class.is_some());
            let bird_class = bird_class.unwrap();
            assert!(
                bird_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("with Flyable")
            );

            let duck_class = symbols.iter().find(|s| s.name == "Duck");
            assert!(duck_class.is_some());
            let duck_class = duck_class.unwrap();
            assert!(
                duck_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("with Flyable, Swimmable")
            );

            // Should extract extensions
            let string_extension = symbols.iter().find(|s| s.name == "StringExtensions");
            assert!(string_extension.is_some());
            let string_extension = string_extension.unwrap();
            assert!(
                string_extension
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("extension StringExtensions on String")
            );

            // Should extract extension methods
            let capitalized_getter = symbols.iter().find(|s| s.name == "capitalized");
            assert!(capitalized_getter.is_some());

            let is_email_getter = symbols.iter().find(|s| s.name == "isEmail");
            assert!(is_email_getter.is_some());

            let reverse_method = symbols.iter().find(|s| s.name == "reverse");
            assert!(reverse_method.is_some());
        }
    }

    mod enums_and_functions {
        use super::*;

        #[test]
        fn test_extract_enums_and_top_level_functions() {
            let code = r#"
enum Color {
  red('Red'),
  green('Green'),
  blue('Blue');

  const Color(this.displayName);
  final String displayName;

  static Color fromHex(String hex) {
    switch (hex) {
      case '#FF0000': return Color.red;
      case '#00FF00': return Color.green;
      case '#0000FF': return Color.blue;
      default: throw ArgumentError('Invalid hex: $hex');
    }
  }
}

enum Status { pending, approved, rejected }

// Top-level functions
String formatName(String first, String last, {String? middle}) {
  return middle != null ? '$first $middle $last' : '$first $last';
}

Future<String> fetchUserData(int userId) async {
  await Future.delayed(Duration(seconds: 1));
  return 'User data for $userId';
}

Stream<int> countStream() async* {
  for (int i = 0; i < 10; i++) {
    yield i;
    await Future.delayed(Duration(milliseconds: 100));
  }
}

typedef StringCallback = void Function(String);
typedef NumberProcessor<T extends num> = T Function(T);

T processData<T extends Comparable<T>>(T data, T Function(T) processor) {
  return processor(data);
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract enums
            let color_enum = symbols
                .iter()
                .find(|s| s.name == "Color" && s.kind == SymbolKind::Enum);
            assert!(color_enum.is_some());
            let color_enum = color_enum.unwrap();
            assert!(
                color_enum
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("enum Color")
            );

            let status_enum = symbols
                .iter()
                .find(|s| s.name == "Status" && s.kind == SymbolKind::Enum);
            assert!(status_enum.is_some());

            // Should extract enum members
            let red_member = symbols.iter().find(|s| s.name == "red");
            assert!(red_member.is_some());

            let green_member = symbols.iter().find(|s| s.name == "green");
            assert!(green_member.is_some());

            // Should extract enum constructor and method
            let color_constructor = symbols
                .iter()
                .find(|s| s.name == "Color" && s.kind == SymbolKind::Constructor);
            assert!(color_constructor.is_some());

            let from_hex_method = symbols.iter().find(|s| s.name == "fromHex");
            assert!(from_hex_method.is_some());
            let from_hex_method = from_hex_method.unwrap();
            assert!(
                from_hex_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("static")
            );

            // Should extract top-level functions
            let format_name_function = symbols
                .iter()
                .find(|s| s.name == "formatName" && s.kind == SymbolKind::Function);
            assert!(format_name_function.is_some());
            let format_name_function = format_name_function.unwrap();
            assert!(
                format_name_function
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("String formatName")
            );

            let fetch_user_data_function = symbols.iter().find(|s| s.name == "fetchUserData");
            assert!(fetch_user_data_function.is_some());
            let fetch_user_data_function = fetch_user_data_function.unwrap();
            assert!(
                fetch_user_data_function
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("Future<String>")
            );
            assert!(
                fetch_user_data_function
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("async")
            );

            let count_stream_function = symbols.iter().find(|s| s.name == "countStream");
            assert!(count_stream_function.is_some());
            let count_stream_function = count_stream_function.unwrap();
            assert!(
                count_stream_function
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("Stream<int>")
            );

            // Should extract generic function
            let process_data_function = symbols.iter().find(|s| s.name == "processData");
            assert!(process_data_function.is_some());
            let process_data_function = process_data_function.unwrap();
            assert!(
                process_data_function
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("<T extends Comparable<T>>")
            );

            // Should extract typedefs
            let string_callback_typedef = symbols.iter().find(|s| s.name == "StringCallback");
            assert!(string_callback_typedef.is_some());
            let string_callback_typedef = string_callback_typedef.unwrap();
            assert!(
                string_callback_typedef
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("typedef")
            );

            let number_processor_typedef = symbols.iter().find(|s| s.name == "NumberProcessor");
            assert!(number_processor_typedef.is_some());
            let number_processor_typedef = number_processor_typedef.unwrap();
            assert!(
                number_processor_typedef
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("typedef")
            );
            assert!(
                number_processor_typedef
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("<T extends num>")
            );
        }
    }

    mod flutter_widget_patterns {
        use super::*;

        #[test]
        fn test_extract_flutter_widget_classes_and_methods() {
            let code = r#"
import 'package:flutter/material.dart';

class MyHomePage extends StatefulWidget {
  final String title;

  const MyHomePage({Key? key, required this.title}) : super(key: key);

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}

class _MyHomePageState extends State<MyHomePage> with TickerProviderStateMixin {
  int _counter = 0;
  late AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(vsync: this, duration: Duration(seconds: 1));
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _incrementCounter() {
    setState(() {
      _counter++;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(widget.title)),
      body: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text('You have pushed the button this many times:'),
            Text('$_counter', style: Theme.of(context).textTheme.headlineMedium),
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: _incrementCounter,
        tooltip: 'Increment',
        child: Icon(Icons.add),
      ),
    );
  }
}

class CustomButton extends StatelessWidget {
  final VoidCallback? onPressed;
  final String text;
  final Color? color;

  const CustomButton({
    Key? key,
    this.onPressed,
    required this.text,
    this.color,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) => ElevatedButton(
    onPressed: onPressed,
    style: ElevatedButton.styleFrom(backgroundColor: color),
    child: Text(text),
  );
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract Flutter widget classes
            let my_home_page_class = symbols.iter().find(|s| s.name == "MyHomePage");
            assert!(my_home_page_class.is_some());
            let my_home_page_class = my_home_page_class.unwrap();
            assert!(
                my_home_page_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("extends StatefulWidget")
            );

            let state_class = symbols.iter().find(|s| s.name == "_MyHomePageState");
            assert!(state_class.is_some());
            let state_class = state_class.unwrap();
            assert!(
                state_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("extends State<MyHomePage>")
            );
            assert!(
                state_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("with TickerProviderStateMixin")
            );

            let custom_button_class = symbols.iter().find(|s| s.name == "CustomButton");
            assert!(custom_button_class.is_some());
            let custom_button_class = custom_button_class.unwrap();
            assert!(
                custom_button_class
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("extends StatelessWidget")
            );

            // Should extract lifecycle methods
            let init_state_method = symbols.iter().find(|s| s.name == "initState");
            assert!(init_state_method.is_some());
            let init_state_method = init_state_method.unwrap();
            assert!(
                init_state_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("@override")
            );

            let dispose_method = symbols.iter().find(|s| s.name == "dispose");
            assert!(dispose_method.is_some());

            // Should extract build methods
            let build_methods: Vec<_> = symbols.iter().filter(|s| s.name == "build").collect();
            assert_eq!(build_methods.len(), 2); // One for each widget

            let home_page_build = build_methods.iter().find(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("Widget build"))
            });
            assert!(home_page_build.is_some());

            // Should extract custom methods
            let increment_method = symbols.iter().find(|s| s.name == "_incrementCounter");
            assert!(increment_method.is_some());
            let increment_method = increment_method.unwrap();
            assert_eq!(
                increment_method.visibility,
                Some(crate::base::Visibility::Private)
            );

            // Should extract createState method
            let create_state_method = symbols.iter().find(|s| s.name == "createState");
            assert!(create_state_method.is_some());
            let create_state_method = create_state_method.unwrap();
            assert!(
                create_state_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("@override")
            );

            // Should extract fields
            let title_field = symbols.iter().find(|s| s.name == "title");
            assert!(title_field.is_some());
            let title_field = title_field.unwrap();
            assert!(
                title_field
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("final String title")
            );

            let counter_field = symbols.iter().find(|s| s.name == "_counter");
            assert!(counter_field.is_some());
            let counter_field = counter_field.unwrap();
            assert_eq!(
                counter_field.visibility,
                Some(crate::base::Visibility::Private)
            );

            let controller_field = symbols.iter().find(|s| s.name == "_controller");
            assert!(controller_field.is_some());
            let controller_field = controller_field.unwrap();
            assert!(
                controller_field
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("late AnimationController")
            );
        }
    }

    mod type_inference_and_relationships {
        use super::*;

        #[test]
        fn test_infer_types_and_extract_relationships() {
            let code = r#"
abstract class Shape {
  double get area;
  String describe() => 'A shape with area ${area}';
}

class Rectangle extends Shape {
  final double width;
  final double height;

  Rectangle(this.width, this.height);

  @override
  double get area => width * height;
}

class Circle extends Shape {
  final double radius;

  Circle(this.radius);

  @override
  double get area => 3.14159 * radius * radius;
}

mixin ColoredMixin {
  Color? color;
  void setColor(Color newColor) => color = newColor;
}

class ColoredRectangle extends Rectangle with ColoredMixin {
  ColoredRectangle(double width, double height) : super(width, height);
}

// Generic class
class Container<T> {
  late T _value;

  Container(this._value);

  T get value => _value;
  set value(T newValue) => _value = newValue;

  void process<R>(R Function(T) processor) {
    // Process the value
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let relationships = extractor.extract_relationships(&tree, &symbols);
            let types = extractor.infer_types(&symbols);

            // Should extract inheritance relationships
            assert!(relationships.len() > 0);

            let rectangle_inheritance = relationships.iter().find(|r| {
                r.kind == crate::base::RelationshipKind::Extends && {
                    let from_symbol = symbols.iter().find(|s| s.id == r.from_symbol_id);
                    from_symbol.map_or(false, |s| s.name == "Rectangle")
                }
            });
            assert!(rectangle_inheritance.is_some());

            let circle_inheritance = relationships.iter().find(|r| {
                r.kind == crate::base::RelationshipKind::Extends && {
                    let from_symbol = symbols.iter().find(|s| s.id == r.from_symbol_id);
                    from_symbol.map_or(false, |s| s.name == "Circle")
                }
            });
            assert!(circle_inheritance.is_some());

            // Should extract mixin relationships
            let mixin_relationship = relationships.iter().find(|r| {
                r.kind == crate::base::RelationshipKind::Uses && {
                    let from_symbol = symbols.iter().find(|s| s.id == r.from_symbol_id);
                    from_symbol.map_or(false, |s| s.name == "ColoredRectangle")
                }
            });
            assert!(mixin_relationship.is_some());

            // Should infer types
            assert!(types.len() > 0);

            // Should identify generic types
            let container_class = symbols.iter().find(|s| s.name == "Container");
            assert!(container_class.is_some());
            let container_class = container_class.unwrap();
            assert!(container_class.signature.as_ref().unwrap().contains("<T>"));

            let process_method = symbols.iter().find(|s| s.name == "process");
            assert!(process_method.is_some());
            let process_method = process_method.unwrap();
            assert!(process_method.signature.as_ref().unwrap().contains("<R>"));

            // Should handle getter/setter pairs
            let value_getter = symbols.iter().find(|s| {
                s.name == "value"
                    && s.signature
                        .as_ref()
                        .map_or(false, |sig| sig.contains("get"))
            });
            assert!(value_getter.is_some());

            let value_setter = symbols.iter().find(|s| {
                s.name == "value"
                    && s.signature
                        .as_ref()
                        .map_or(false, |sig| sig.contains("set"))
            });
            assert!(value_setter.is_some());
        }
    }

    // ========================================================================
    // Identifier Extraction Tests (TDD RED phase)
    // ========================================================================
    //
    // These tests validate the extract_identifiers() functionality which extracts:
    // - Function/method calls (method_invocation)
    // - Member access (selector, unconditional_assignable_selector)
    // - Proper containing symbol tracking (file-scoped)
    //
    // Following the Rust/C# extractor reference implementation pattern

    mod identifier_extraction {
        use super::*;
        use crate::base::IdentifierKind;

        #[test]
        fn test_dart_function_calls() {
            let dart_code = r#"
class Calculator {
  int add(int a, int b) {
    return a + b;
  }

  int calculate() {
    int result = add(5, 3);      // Function call to add
    print(result);                // Function call to print
    return result;
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(dart_code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                dart_code.to_string(),
                &workspace_root,
            );

            // Extract symbols first
            let symbols = extractor.extract_symbols(&tree);

            // NOW extract identifiers (this will FAIL until we implement it)
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Verify we found the function calls
            let add_call = identifiers.iter().find(|id| id.name == "add");
            assert!(
                add_call.is_some(),
                "Should extract 'add' function call identifier"
            );
            let add_call = add_call.unwrap();
            assert_eq!(add_call.kind, IdentifierKind::Call);

            let print_call = identifiers.iter().find(|id| id.name == "print");
            assert!(
                print_call.is_some(),
                "Should extract 'print' function call identifier"
            );
            let print_call = print_call.unwrap();
            assert_eq!(print_call.kind, IdentifierKind::Call);

            // Verify containing symbol is set correctly
            assert!(
                add_call.containing_symbol_id.is_some(),
                "Function call should have containing symbol"
            );

            // NOTE: Due to Dart extractor limitation, methods only capture signature lines,
            // not full method bodies. This means the call may be contained within the class
            // instead of the specific method. This is expected behavior for Dart.
            //
            // Verify the containing symbol is from the SAME FILE (file-scoped filtering)
            let containing_symbol = symbols
                .iter()
                .find(|s| Some(&s.id) == add_call.containing_symbol_id.as_ref());
            assert!(
                containing_symbol.is_some(),
                "Containing symbol should exist in symbols list"
            );
            assert_eq!(
                containing_symbol.unwrap().file_path,
                "test.dart",
                "Containing symbol should be from same file (file-scoped filtering)"
            );
        }

        #[test]
        fn test_dart_member_access() {
            let dart_code = r#"
class User {
  String name;
  String email;

  void printInfo() {
    print(this.name);   // Member access: this.name
    var e = this.email; // Member access: this.email
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(dart_code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                dart_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Verify we found member access identifiers
            let name_access = identifiers
                .iter()
                .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
                .count();
            assert!(
                name_access > 0,
                "Should extract 'name' member access identifier"
            );

            let email_access = identifiers
                .iter()
                .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
                .count();
            assert!(
                email_access > 0,
                "Should extract 'email' member access identifier"
            );
        }

        #[test]
        fn test_dart_identifiers_have_containing_symbol() {
            // This test ensures we ONLY match symbols from the SAME FILE
            // Critical bug fix from Rust implementation
            let dart_code = r#"
class Service {
  void process() {
    helper();              // Call to helper in same file
  }

  void helper() {
    // Helper method
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(dart_code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                dart_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Find the helper call
            let helper_call = identifiers.iter().find(|id| id.name == "helper");
            assert!(helper_call.is_some());
            let helper_call = helper_call.unwrap();

            // Verify it has a containing symbol from same file
            assert!(
                helper_call.containing_symbol_id.is_some(),
                "helper call should have containing symbol from same file"
            );

            // NOTE: Due to Dart extractor limitation, methods only capture signature lines.
            // Verify the containing symbol is from the SAME FILE (file-scoped filtering)
            let containing_symbol = symbols
                .iter()
                .find(|s| Some(&s.id) == helper_call.containing_symbol_id.as_ref());
            assert!(
                containing_symbol.is_some(),
                "Containing symbol should exist in symbols list"
            );
            assert_eq!(
                containing_symbol.unwrap().file_path,
                "test.dart",
                "Containing symbol must be from same file (file-scoped filtering works correctly)"
            );
        }

        #[test]
        fn test_dart_chained_member_access() {
            let dart_code = r#"
class DataService {
  void execute() {
    var result = user.account.balance;   // Chained member access
    var name = customer.profile.name;     // Chained member access
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(dart_code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                dart_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Should extract the rightmost identifiers in chains
            let balance_access = identifiers
                .iter()
                .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                balance_access.is_some(),
                "Should extract 'balance' from chained member access"
            );

            let name_access = identifiers
                .iter()
                .find(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                name_access.is_some(),
                "Should extract 'name' from chained member access"
            );
        }

        #[test]
        fn test_dart_duplicate_calls_at_different_locations() {
            let dart_code = r#"
class Test {
  void run() {
    process();
    process();  // Same call twice
  }

  void process() {
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(dart_code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                dart_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Should extract BOTH calls (they're at different locations)
            let process_calls: Vec<_> = identifiers
                .iter()
                .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
                .collect();

            assert_eq!(
                process_calls.len(),
                2,
                "Should extract both process calls at different locations"
            );

            // Verify they have different line numbers
            assert_ne!(
                process_calls[0].start_line, process_calls[1].start_line,
                "Duplicate calls should have different line numbers"
            );
        }
    }

    mod async_await_and_futures {
        use super::*;

        #[test]
        fn test_extract_async_functions_and_futures() {
            let code = r#"
// Async function
Future<String> fetchUserData(int userId) async {
  try {
    var response = await http.get(Uri.parse('https://api.example.com/users/$userId'));
    return response.body;
  } catch (e) {
    return 'Error: $e';
  }
}

// Stream function
Stream<int> countDown(int from) async* {
  for (int i = from; i >= 0; i--) {
    yield i;
    await Future.delayed(Duration(seconds: 1));
  }
}

// Future with completer
Future<String> processDataWithCompleter() {
  var completer = Completer<String>();

  Timer(Duration(seconds: 2), () {
    completer.complete('Data processed');
  });

  return completer.future;
}

// Async generator
Stream<String> generateMessages() async* {
  var messages = ['Hello', 'World', 'from', 'Dart'];

  for (var message in messages) {
    yield message;
    await Future.delayed(Duration(milliseconds: 500));
  }
}

// Future chaining
Future<String> chainOperations() {
  return fetchUserData(123)
      .then((data) => data.toUpperCase())
      .then((upperData) => 'Processed: $upperData')
      .catchError((error) => 'Failed: $error');
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "async.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Test async functions
            let fetch_user_data = symbols.iter().find(|s| s.name == "fetchUserData");
            assert!(fetch_user_data.is_some());
            assert_eq!(fetch_user_data.unwrap().kind, SymbolKind::Function);

            let count_down = symbols.iter().find(|s| s.name == "countDown");
            assert!(count_down.is_some());

            let process_data_with_completer = symbols
                .iter()
                .find(|s| s.name == "processDataWithCompleter");
            assert!(process_data_with_completer.is_some());

            let generate_messages = symbols.iter().find(|s| s.name == "generateMessages");
            assert!(generate_messages.is_some());

            let chain_operations = symbols.iter().find(|s| s.name == "chainOperations");
            assert!(chain_operations.is_some());
        }
    }

    mod error_handling_and_exceptions {
        use super::*;

        #[test]
        fn test_extract_exception_handling_patterns() {
            let code = r#"
// Custom exception class
class NetworkException implements Exception {
  final String message;
  final int statusCode;

  NetworkException(this.message, this.statusCode);

  @override
  String toString() => 'NetworkException: $message (Status: $statusCode)';
}

// Function with try-catch
Future<String> safeApiCall(String url) async {
  try {
    var response = await http.get(Uri.parse(url));
    if (response.statusCode != 200) {
      throw NetworkException('API call failed', response.statusCode);
    }
    return response.body;
  } on SocketException catch (e) {
    throw NetworkException('Network error: ${e.message}', 0);
  } on TimeoutException catch (e) {
    throw NetworkException('Request timeout', 408);
  } catch (e) {
    throw NetworkException('Unknown error: $e', 500);
  } finally {
    print('API call completed');
  }
}

// Rethrow pattern
void rethrowExample() {
  try {
    riskyOperation();
  } catch (e) {
    logError(e);
    rethrow;
  }
}

// Custom error handler
class ErrorHandler {
  static void handleError(dynamic error, StackTrace stackTrace) {
    print('Error: $error');
    print('Stack trace: $stackTrace');

    if (error is NetworkException) {
      // Handle network errors
      reportToAnalytics(error);
    } else {
      // Handle other errors
      sendToCrashReporting(error, stackTrace);
    }
  }
}

// Zone with error handling
void runWithErrorHandling(void Function() operation) {
  runZonedGuarded(() {
    operation();
  }, (error, stackTrace) {
    ErrorHandler.handleError(error, stackTrace);
  });
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "errors.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Test exception classes
            let network_exception = symbols.iter().find(|s| s.name == "NetworkException");
            assert!(network_exception.is_some());
            assert_eq!(network_exception.unwrap().kind, SymbolKind::Class);

            // Test error handling functions
            let safe_api_call = symbols.iter().find(|s| s.name == "safeApiCall");
            assert!(safe_api_call.is_some());

            let rethrow_example = symbols.iter().find(|s| s.name == "rethrowExample");
            assert!(rethrow_example.is_some());

            // Test error handler class
            let error_handler = symbols.iter().find(|s| s.name == "ErrorHandler");
            assert!(error_handler.is_some());
            assert_eq!(error_handler.unwrap().kind, SymbolKind::Class);

            let run_with_error_handling = symbols.iter().find(|s| s.name == "runWithErrorHandling");
            assert!(run_with_error_handling.is_some());
        }
    }

    mod isolates_and_concurrency {
        use super::*;

        #[test]
        fn test_extract_isolates_and_concurrent_patterns() {
            let code = r#"
// Isolate function
void isolateFunction(SendPort sendPort) {
  // Perform computation in isolate
  var result = heavyComputation();
  sendPort.send(result);
}

// Spawn isolate
Future<void> runInIsolate() async {
  var receivePort = ReceivePort();

  await Isolate.spawn(isolateFunction, receivePort.sendPort);

  var result = await receivePort.first;
  print('Result from isolate: $result');
  receivePort.close();
}

// Compute-intensive function
int heavyComputation() {
  var sum = 0;
  for (var i = 0; i < 1000000; i++) {
    sum += i;
  }
  return sum;
}

// Message passing between isolates
class IsolateMessenger {
  final SendPort _sendPort;

  IsolateMessenger(this._sendPort);

  void sendMessage(dynamic message) {
    _sendPort.send(message);
  }
}

// Isolate with message handling
void messageHandlingIsolate(SendPort sendPort) {
  var receivePort = ReceivePort();
  sendPort.send(receivePort.sendPort);

  receivePort.listen((message) {
    if (message == 'exit') {
      receivePort.close();
      return;
    }

    // Process message
    var result = processMessage(message);
    sendPort.send(result);
  });
}

// Concurrent data processing
Future<List<String>> processDataConcurrently(List<String> data) async {
  var chunkSize = (data.length / 4).ceil();
  var chunks = <List<String>>[];

  for (var i = 0; i < data.length; i += chunkSize) {
    var end = (i + chunkSize < data.length) ? i + chunkSize : data.length;
    chunks.add(data.sublist(i, end));
  }

  var futures = chunks.map((chunk) => Isolate.run(() => processChunk(chunk)));
  var results = await Future.wait(futures);

  return results.expand((x) => x).toList();
}

List<String> processChunk(List<String> chunk) {
  return chunk.map((item) => 'Processed: $item').toList();
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "isolates.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Test isolate functions
            let isolate_function = symbols.iter().find(|s| s.name == "isolateFunction");
            assert!(isolate_function.is_some());

            let run_in_isolate = symbols.iter().find(|s| s.name == "runInIsolate");
            assert!(run_in_isolate.is_some());

            let heavy_computation = symbols.iter().find(|s| s.name == "heavyComputation");
            assert!(heavy_computation.is_some());

            // Test isolate classes
            let isolate_messenger = symbols.iter().find(|s| s.name == "IsolateMessenger");
            assert!(isolate_messenger.is_some());
            assert_eq!(isolate_messenger.unwrap().kind, SymbolKind::Class);

            let message_handling_isolate =
                symbols.iter().find(|s| s.name == "messageHandlingIsolate");
            assert!(message_handling_isolate.is_some());

            let process_data_concurrently =
                symbols.iter().find(|s| s.name == "processDataConcurrently");
            assert!(process_data_concurrently.is_some());

            let process_chunk = symbols.iter().find(|s| s.name == "processChunk");
            assert!(process_chunk.is_some());
        }
    }

    mod streams_and_reactive_patterns {
        use super::*;

        #[test]
        fn test_extract_streams_and_rx_patterns() {
            let code = r#"
// Basic stream
Stream<int> countStream(int max) async* {
  for (int i = 0; i < max; i++) {
    yield i;
    await Future.delayed(Duration(milliseconds: 100));
  }
}

// Stream transformation
Stream<String> transformStream(Stream<int> input) {
  return input
      .where((number) => number % 2 == 0)
      .map((number) => 'Even: $number')
      .take(5);
}

// Stream controller
class NumberStreamController {
  final _controller = StreamController<int>();

  Stream<int> get stream => _controller.stream;

  void addNumber(int number) {
    _controller.add(number);
  }

  void close() {
    _controller.close();
  }
}

// Broadcast stream
class EventBus {
  final _controller = StreamController<String>.broadcast();

  Stream<String> get onEvent => _controller.stream;

  void fireEvent(String event) {
    _controller.add(event);
  }

  void dispose() {
    _controller.close();
  }
}

// Stream subscription management
class StreamManager {
  final List<StreamSubscription> _subscriptions = [];

  void addSubscription(StreamSubscription subscription) {
    _subscriptions.add(subscription);
  }

  void cancelAll() {
    for (var subscription in _subscriptions) {
      subscription.cancel();
    }
    _subscriptions.clear();
  }
}

// Reactive pattern with rxdart
Stream<int> reactiveCounter(Stream<void> incrementStream) {
  return incrementStream
      .scan<int>((accumulator, _, __) => accumulator + 1, 0)
      .startWith(0);
}

// Error handling in streams
Stream<String> safeStream() async* {
  try {
    yield 'Starting';
    yield await riskyOperation();
    yield 'Completed';
  } catch (e) {
    yield 'Error: $e';
  }
}

Future<String> riskyOperation() async {
  await Future.delayed(Duration(seconds: 1));
  if (Random().nextBool()) {
    throw Exception('Random failure');
  }
  return 'Success';
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "streams.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Test stream functions
            let count_stream = symbols.iter().find(|s| s.name == "countStream");
            assert!(count_stream.is_some());

            let transform_stream = symbols.iter().find(|s| s.name == "transformStream");
            assert!(transform_stream.is_some());

            // Test stream classes
            let number_stream_controller =
                symbols.iter().find(|s| s.name == "NumberStreamController");
            assert!(number_stream_controller.is_some());
            assert_eq!(number_stream_controller.unwrap().kind, SymbolKind::Class);

            let event_bus = symbols.iter().find(|s| s.name == "EventBus");
            assert!(event_bus.is_some());
            assert_eq!(event_bus.unwrap().kind, SymbolKind::Class);

            let stream_manager = symbols.iter().find(|s| s.name == "StreamManager");
            assert!(stream_manager.is_some());
            assert_eq!(stream_manager.unwrap().kind, SymbolKind::Class);

            // Test reactive functions
            let reactive_counter = symbols.iter().find(|s| s.name == "reactiveCounter");
            assert!(reactive_counter.is_some());

            let safe_stream = symbols.iter().find(|s| s.name == "safeStream");
            assert!(safe_stream.is_some());

            let risky_operation = symbols.iter().find(|s| s.name == "riskyOperation");
            assert!(risky_operation.is_some());
        }
    }

    mod annotations_and_metadata {
        use super::*;

        #[test]
        fn test_extract_annotations_and_metadata() {
            let code = r#"
// Built-in annotations
@deprecated
@override
void oldMethod() {
  print('This method is deprecated');
}

class User {
  final String name;
  final int age;

  @JsonKey(name: 'user_name')
  final String userName;

  @JsonKey(ignore: true)
  final String password;

  User({
    required this.name,
    required this.age,
    required this.userName,
    required this.password,
  });

  @JsonSerializable()
  factory User.fromJson(Map<String, dynamic> json) => _$UserFromJson(json);

  @JsonSerializable()
  Map<String, dynamic> toJson() => _$UserToJson(this);
}

// Custom annotation
class Todo {
  final String message;
  final String assignee;
  const Todo(this.message, {this.assignee = 'unassigned'});
}

// Using custom annotation
@Todo('Implement caching', assignee: 'developer')
class CacheManager {
  @Todo('Add cache invalidation')
  void clearCache() {
    // Implementation
  }

  @Todo('Optimize cache size calculation')
  int getCacheSize() {
    return 0;
  }
}

// Reflection-like metadata
class Metadata {
  final Map<String, dynamic> data;

  const Metadata(this.data);

  dynamic get(String key) => data[key];
}

@Metadata({'version': '1.0', 'author': 'team', 'deprecated': false})
class ApiService {
  @Metadata({'httpMethod': 'GET', 'path': '/users'})
  Future<List<User>> getUsers() async {
    // Implementation
    return [];
  }

  @Metadata({'httpMethod': 'POST', 'path': '/users', 'requiresAuth': true})
  Future<User> createUser(@Metadata({'fromBody': true}) User user) async {
    // Implementation
    return user;
  }
}

// Annotation for dependency injection
class Injectable {
  const Injectable();
}

@Service()
class UserService {
  @Injectable()
  final Database database;

  UserService(this.database);

  @Transactional()
  Future<void> saveUser(User user) async {
    // Implementation
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "annotations.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Test annotated functions
            let old_method = symbols.iter().find(|s| s.name == "oldMethod");
            assert!(old_method.is_some());

            // Test annotated classes
            let user = symbols.iter().find(|s| s.name == "User");
            assert!(user.is_some());
            assert_eq!(user.unwrap().kind, SymbolKind::Class);

            // Test custom annotation class
            let todo = symbols.iter().find(|s| s.name == "Todo");
            assert!(todo.is_some());
            assert_eq!(todo.unwrap().kind, SymbolKind::Class);

            // Test annotated class
            let cache_manager = symbols.iter().find(|s| s.name == "CacheManager");
            assert!(cache_manager.is_some());
            assert_eq!(cache_manager.unwrap().kind, SymbolKind::Class);

            // Test metadata class
            let metadata = symbols.iter().find(|s| s.name == "Metadata");
            assert!(metadata.is_some());
            assert_eq!(metadata.unwrap().kind, SymbolKind::Class);

            // Test annotated service class
            let api_service = symbols.iter().find(|s| s.name == "ApiService");
            assert!(api_service.is_some());
            assert_eq!(api_service.unwrap().kind, SymbolKind::Class);

            let user_service = symbols.iter().find(|s| s.name == "UserService");
            assert!(user_service.is_some());
            assert_eq!(user_service.unwrap().kind, SymbolKind::Class);
        }
    }

    mod doc_comments {
        use super::*;

        #[test]
        fn test_extract_class_with_single_line_dartdoc() {
            let code = r#"
/// UserService manages user authentication and login
class UserService {
    void authenticate() {}
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let user_service = symbols
                .iter()
                .find(|s| s.name == "UserService" && s.kind == SymbolKind::Class);
            assert!(user_service.is_some(), "Should extract UserService class");
            let user_service = user_service.unwrap();
            assert!(
                user_service.doc_comment.is_some(),
                "Should have dartdoc comment"
            );
            assert!(
                user_service
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("manages user authentication"),
                "Doc comment should contain the documentation text"
            );
        }

        #[test]
        fn test_extract_function_with_block_dartdoc() {
            let code = r#"
/// Validates user credentials
///
/// Parameters:
/// - [username] is the username to validate
/// - [password] is the password to check
/// Returns `true` if credentials are valid
bool validateCredentials(String username, String password) {
    return true;
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let validate_func = symbols
                .iter()
                .find(|s| s.name == "validateCredentials" && s.kind == SymbolKind::Function);
            assert!(
                validate_func.is_some(),
                "Should extract validateCredentials function"
            );
            let validate_func = validate_func.unwrap();
            assert!(
                validate_func.doc_comment.is_some(),
                "Should have dartdoc comment"
            );
            let doc = validate_func.doc_comment.as_ref().unwrap();
            assert!(
                doc.contains("Validates user credentials"),
                "Doc should contain main description"
            );
            assert!(
                doc.contains("[username]"),
                "Doc should contain parameter documentation with [param] syntax"
            );
            assert!(
                doc.contains("[password]"),
                "Doc should contain second parameter documentation"
            );
        }

        #[test]
        fn test_extract_method_with_dartdoc() {
            let code = r#"
class Calculator {
    /// Adds two numbers together
    ///
    /// Returns the sum of [a] and [b]
    int add(int a, int b) {
        return a + b;
    }

    /// Multiplies two numbers
    ///
    /// Returns the product of [x] and [y]
    int multiply(int x, int y) => x * y;
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let add_method = symbols
                .iter()
                .find(|s| s.name == "add" && s.kind == SymbolKind::Method);
            assert!(add_method.is_some(), "Should extract add method");
            let add_method = add_method.unwrap();
            assert!(
                add_method.doc_comment.is_some(),
                "Should have dartdoc comment"
            );
            let doc = add_method.doc_comment.as_ref().unwrap();
            assert!(
                doc.contains("Adds two numbers"),
                "Doc should contain main description"
            );
            assert!(
                doc.contains("[a]") && doc.contains("[b]"),
                "Doc should contain parameter references"
            );

            let multiply_method = symbols
                .iter()
                .find(|s| s.name == "multiply" && s.kind == SymbolKind::Method);
            assert!(multiply_method.is_some(), "Should extract multiply method");
            let multiply_method = multiply_method.unwrap();
            assert!(
                multiply_method.doc_comment.is_some(),
                "Should have dartdoc for multiply"
            );
        }

        #[test]
        fn test_extract_property_with_dartdoc() {
            let code = r#"
class Server {
    /// The server hostname
    String _hostname = 'localhost';

    /// Get the hostname for this server
    String get hostname => _hostname;

    /// Set a new hostname for this server
    set hostname(String value) {
        _hostname = value;
    }

    /// Whether the server is running
    bool isRunning = false;
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let hostname_getter = symbols
                .iter()
                .find(|s| s.name == "hostname" && s.kind == SymbolKind::Property);
            assert!(
                hostname_getter.is_some(),
                "Should extract hostname getter property"
            );
            let hostname_getter = hostname_getter.unwrap();
            assert!(
                hostname_getter.doc_comment.is_some(),
                "Should have dartdoc comment"
            );
            assert!(
                hostname_getter
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("hostname"),
                "Doc should contain getter description"
            );
        }

        #[test]
        fn test_extract_constructor_with_dartdoc() {
            let code = r#"
class Person {
    String name;
    int age;

    /// Creates a new Person instance
    ///
    /// The [name] parameter is required
    /// The [age] parameter must be non-negative
    Person(this.name, this.age);

    /// Creates a Person from JSON data
    ///
    /// Parses [json] map and creates a new instance
    Person.fromJson(Map<String, dynamic> json)
        : name = json['name'],
          age = json['age'];
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Check default constructor has doc comment
            let person_constructor = symbols
                .iter()
                .find(|s| s.name == "Person" && s.kind == SymbolKind::Constructor);
            assert!(
                person_constructor.is_some(),
                "Should extract default constructor"
            );
            let person_constructor = person_constructor.unwrap();
            assert!(
                person_constructor.doc_comment.is_some(),
                "Should have dartdoc comment for constructor"
            );
            assert!(
                person_constructor
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("Creates a new Person"),
                "Doc should contain constructor description"
            );

            // Check named constructor has doc comment
            let from_json_constructor = symbols
                .iter()
                .find(|s| s.name.contains("fromJson") && s.kind == SymbolKind::Constructor);
            assert!(
                from_json_constructor.is_some(),
                "Should extract fromJson constructor"
            );
            let from_json_constructor = from_json_constructor.unwrap();
            assert!(
                from_json_constructor.doc_comment.is_some(),
                "Should have dartdoc for named constructor"
            );
        }
    }

    mod typedef_kind {
        use super::*;

        #[test]
        fn test_typedef_uses_type_kind() {
            let code = r#"
typedef StringCallback = void Function(String);
typedef NumberProcessor<T extends num> = T Function(T);
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let callback = symbols
                .iter()
                .find(|s| s.name == "StringCallback")
                .expect("Should extract StringCallback typedef");
            assert_eq!(
                callback.kind,
                SymbolKind::Type,
                "typedef should be SymbolKind::Type, not Class"
            );

            let processor = symbols
                .iter()
                .find(|s| s.name == "NumberProcessor")
                .expect("Should extract NumberProcessor typedef");
            assert_eq!(
                processor.kind,
                SymbolKind::Type,
                "generic typedef should also be SymbolKind::Type"
            );
        }
    }

    mod error_recovery {
        use super::*;

        #[test]
        fn test_error_recovery_no_false_positives_from_method_bodies() {
            // Enhanced enum with member accesses in method bodies.
            // The recovery should only extract actual enum values, not
            // identifiers from method bodies (which the misparsed tree
            // may present as member_access nodes).
            let code = r#"
enum Color {
  red('Red'),
  green('Green'),
  blue('Blue');

  const Color(this.displayName);
  final String displayName;

  static Color fromHex(String hex) {
    switch (hex) {
      case '#FF0000': return Color.red;
      case '#00FF00': return Color.green;
      case '#0000FF': return Color.blue;
      default: throw ArgumentError('Invalid hex: $hex');
    }
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let enum_members: Vec<_> = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::EnumMember)
                .collect();

            // Only red, green, blue should be EnumMember -- nothing else
            let expected = ["red", "green", "blue"];
            let false_positives: Vec<_> = enum_members
                .iter()
                .filter(|s| !expected.contains(&s.name.as_str()))
                .map(|s| s.name.as_str())
                .collect();
            assert!(
                false_positives.is_empty(),
                "Should not have false positive EnumMembers: {:?}",
                false_positives
            );
        }

        #[test]
        fn test_error_recovery_skips_non_enum_identifiers() {
            // Enhanced enum with method bodies containing member accesses.
            // The tree-sitter parser misparsees the body after the first enum_constant,
            // pushing it into ERROR/expression_statement nodes. The recovery should
            // NOT extract identifiers from method bodies as EnumMember.
            let code = r#"
enum Vehicle {
  car('Car', 4),
  truck('Truck', 6),
  bike('Bike', 2);

  const Vehicle(this.displayName, this.wheels);
  final String displayName;
  final int wheels;

  String describe() {
    return '$displayName has $wheels wheels';
  }

  static Vehicle fromName(String name) {
    return values.firstWhere((v) => v.displayName == name);
  }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Enum constants should be extracted
            assert!(
                symbols
                    .iter()
                    .any(|s| s.name == "car" && s.kind == SymbolKind::EnumMember),
                "Should extract 'car' enum member"
            );

            // Should NOT extract method parameters or local identifiers as EnumMember
            // "displayName" is a field, not an enum constant
            let false_enum_members: Vec<_> = symbols
                .iter()
                .filter(|s| {
                    s.kind == SymbolKind::EnumMember
                        && !["car", "truck", "bike"].contains(&s.name.as_str())
                })
                .map(|s| s.name.as_str())
                .collect();
            assert!(
                false_enum_members.is_empty(),
                "Should not extract non-enum identifiers as EnumMember, but found: {:?}",
                false_enum_members
            );
        }
    }

    mod imports {
        use super::*;

        #[test]
        fn test_extract_dart_core_import() {
            let code = r#"import 'dart:async';

class MyApp {}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let import_sym = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Import && s.name == "dart:async");
            assert!(
                import_sym.is_some(),
                "Should extract import with URI as name. Found symbols: {:?}",
                symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
            );
            let import_sym = import_sym.unwrap();
            assert!(
                import_sym
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("import 'dart:async'"),
                "Signature should contain full directive text"
            );
        }

        #[test]
        fn test_extract_package_import() {
            let code = r#"import 'package:flutter/material.dart';

class MyWidget {}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let import_sym = symbols.iter().find(|s| {
                s.kind == SymbolKind::Import && s.name == "package:flutter/material.dart"
            });
            assert!(
                import_sym.is_some(),
                "Should extract package import. Found symbols: {:?}",
                symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
            );
        }

        #[test]
        fn test_extract_export_directive() {
            let code = r#"export 'src/widgets.dart';

class MyApp {}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let export_sym = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Export && s.name == "src/widgets.dart");
            assert!(
                export_sym.is_some(),
                "Should extract export with URI as name. Found symbols: {:?}",
                symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
            );
            let export_sym = export_sym.unwrap();
            assert!(
                export_sym
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("export 'src/widgets.dart'"),
                "Signature should contain full directive text"
            );
        }

        #[test]
        fn test_extract_multiple_imports() {
            let code = r#"import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
export 'package:foo/bar.dart';

class MyApp {
  void run() {}
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let imports: Vec<_> = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Import)
                .collect();
            assert_eq!(imports.len(), 3, "Should extract 3 imports");

            let exports: Vec<_> = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Export)
                .collect();
            assert_eq!(exports.len(), 1, "Should extract 1 export");

            // Verify none have parent_id (imports are top-level)
            for imp in &imports {
                assert!(
                    imp.parent_id.is_none(),
                    "Import {} should have no parent",
                    imp.name
                );
            }
        }

        #[test]
        fn test_imports_coexist_with_existing_symbols() {
            // Ensure existing Flutter test still works — the import was always
            // in the code, it just wasn't extracted before
            let code = r#"import 'package:flutter/material.dart';

class MyHomePage extends StatefulWidget {
  final String title;

  const MyHomePage({Key? key, required this.title}) : super(key: key);

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}
"#;
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Import should be extracted
            let import_sym = symbols.iter().find(|s| {
                s.kind == SymbolKind::Import && s.name == "package:flutter/material.dart"
            });
            assert!(import_sym.is_some(), "Should extract flutter import");

            // Class should still be extracted
            let class_sym = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Class && s.name == "MyHomePage");
            assert!(class_sym.is_some(), "Should still extract class");
        }

        #[test]
        fn test_import_uri_strips_quotes() {
            let code = "import 'dart:convert';\n";
            let mut parser = init_parser();
            let tree = parser.parse(code, None).unwrap();

            let workspace_root = PathBuf::from("/tmp/test");
            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let import_sym = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Import);
            assert!(import_sym.is_some(), "Should extract import");
            let name = &import_sym.unwrap().name;
            assert!(
                !name.contains('\''),
                "Import name should not contain quotes, got: {}",
                name
            );
            assert_eq!(name, "dart:convert");
        }
    }
}
mod types; // Phase 4: Type extraction verification tests
