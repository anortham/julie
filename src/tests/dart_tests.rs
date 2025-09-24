// Dart Extractor Tests
//
// Direct port of Miller's Dart extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/dart-extractor.test.ts

use crate::extractors::base::{Symbol, SymbolKind, Relationship};
use crate::extractors::dart::DartExtractor;
use tree_sitter::Parser;

/// Initialize Dart parser for Dart files
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&harper_tree_sitter_dart::LANGUAGE.into()).expect("Error loading Dart grammar");
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

            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract classes
            let person_class = symbols.iter()
                .find(|s| s.name == "Person" && s.kind == SymbolKind::Class);
            assert!(person_class.is_some());
            let person_class = person_class.unwrap();
            assert!(person_class.signature.as_ref().unwrap().contains("class Person"));

            let animal_class = symbols.iter()
                .find(|s| s.name == "Animal" && s.kind == SymbolKind::Class);
            assert!(animal_class.is_some());
            let animal_class = animal_class.unwrap();
            assert!(animal_class.signature.as_ref().unwrap().contains("abstract class Animal"));

            let dog_class = symbols.iter()
                .find(|s| s.name == "Dog" && s.kind == SymbolKind::Class);
            assert!(dog_class.is_some());

            // Should extract constructors
            let constructors: Vec<_> = symbols.iter()
                .filter(|s| s.kind == SymbolKind::Constructor)
                .collect();
            assert!(constructors.len() >= 4); // Default, named, factory, const

            let default_constructor = constructors.iter()
                .find(|s| s.name == "Person");
            assert!(default_constructor.is_some());

            let named_constructor = constructors.iter()
                .find(|s| s.name == "Person.baby");
            assert!(named_constructor.is_some());

            let factory_constructor = constructors.iter()
                .find(|s| s.name == "Person.fromJson");
            assert!(factory_constructor.is_some());
            let factory_constructor = factory_constructor.unwrap();
            assert!(factory_constructor.signature.as_ref().unwrap().contains("factory"));

            // Should extract methods
            let greet_method = symbols.iter()
                .find(|s| s.name == "greet");
            assert!(greet_method.is_some());
            let greet_method = greet_method.unwrap();
            assert_eq!(greet_method.kind, SymbolKind::Method);

            let make_sound_method = symbols.iter()
                .find(|s| s.name == "makeSound");
            assert!(make_sound_method.is_some());

            // Should extract getters and setters
            let birth_year_getter = symbols.iter()
                .find(|s| s.name == "birthYear");
            assert!(birth_year_getter.is_some());
            let birth_year_getter = birth_year_getter.unwrap();
            assert!(birth_year_getter.signature.as_ref().unwrap().contains("get"));

            let new_age_setter = symbols.iter()
                .find(|s| s.name == "newAge");
            assert!(new_age_setter.is_some());
            let new_age_setter = new_age_setter.unwrap();
            assert!(new_age_setter.signature.as_ref().unwrap().contains("set"));

            // Should extract fields/properties
            let name_field = symbols.iter()
                .find(|s| s.name == "name");
            assert!(name_field.is_some());
            let name_field = name_field.unwrap();
            assert_eq!(name_field.kind, SymbolKind::Field);

            let total_dogs_field = symbols.iter()
                .find(|s| s.name == "totalDogs");
            assert!(total_dogs_field.is_some());
            let total_dogs_field = total_dogs_field.unwrap();
            assert!(total_dogs_field.signature.as_ref().unwrap().contains("static"));
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

            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract mixins
            let flyable_mixin = symbols.iter()
                .find(|s| s.name == "Flyable");
            assert!(flyable_mixin.is_some());
            let flyable_mixin = flyable_mixin.unwrap();
            assert!(flyable_mixin.signature.as_ref().unwrap().contains("mixin Flyable"));

            let swimmable_mixin = symbols.iter()
                .find(|s| s.name == "Swimmable");
            assert!(swimmable_mixin.is_some());
            let swimmable_mixin = swimmable_mixin.unwrap();
            assert!(swimmable_mixin.signature.as_ref().unwrap().contains("mixin Swimmable on Animal"));

            // Should extract mixin methods
            let fly_method = symbols.iter()
                .find(|s| s.name == "fly");
            assert!(fly_method.is_some());

            let swim_method = symbols.iter()
                .find(|s| s.name == "swim");
            assert!(swim_method.is_some());

            // Should extract classes with mixins
            let bird_class = symbols.iter()
                .find(|s| s.name == "Bird");
            assert!(bird_class.is_some());
            let bird_class = bird_class.unwrap();
            assert!(bird_class.signature.as_ref().unwrap().contains("with Flyable"));

            let duck_class = symbols.iter()
                .find(|s| s.name == "Duck");
            assert!(duck_class.is_some());
            let duck_class = duck_class.unwrap();
            assert!(duck_class.signature.as_ref().unwrap().contains("with Flyable, Swimmable"));

            // Should extract extensions
            let string_extension = symbols.iter()
                .find(|s| s.name == "StringExtensions");
            assert!(string_extension.is_some());
            let string_extension = string_extension.unwrap();
            assert!(string_extension.signature.as_ref().unwrap().contains("extension StringExtensions on String"));

            // Should extract extension methods
            let capitalized_getter = symbols.iter()
                .find(|s| s.name == "capitalized");
            assert!(capitalized_getter.is_some());

            let is_email_getter = symbols.iter()
                .find(|s| s.name == "isEmail");
            assert!(is_email_getter.is_some());

            let reverse_method = symbols.iter()
                .find(|s| s.name == "reverse");
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

            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract enums
            let color_enum = symbols.iter()
                .find(|s| s.name == "Color" && s.kind == SymbolKind::Enum);
            assert!(color_enum.is_some());
            let color_enum = color_enum.unwrap();
            assert!(color_enum.signature.as_ref().unwrap().contains("enum Color"));

            let status_enum = symbols.iter()
                .find(|s| s.name == "Status" && s.kind == SymbolKind::Enum);
            assert!(status_enum.is_some());

            // Should extract enum members
            let red_member = symbols.iter()
                .find(|s| s.name == "red");
            assert!(red_member.is_some());

            let green_member = symbols.iter()
                .find(|s| s.name == "green");
            assert!(green_member.is_some());

            // Should extract enum constructor and method
            let color_constructor = symbols.iter()
                .find(|s| s.name == "Color" && s.kind == SymbolKind::Constructor);
            assert!(color_constructor.is_some());

            let from_hex_method = symbols.iter()
                .find(|s| s.name == "fromHex");
            assert!(from_hex_method.is_some());
            let from_hex_method = from_hex_method.unwrap();
            assert!(from_hex_method.signature.as_ref().unwrap().contains("static"));

            // Should extract top-level functions
            let format_name_function = symbols.iter()
                .find(|s| s.name == "formatName" && s.kind == SymbolKind::Function);
            assert!(format_name_function.is_some());
            let format_name_function = format_name_function.unwrap();
            assert!(format_name_function.signature.as_ref().unwrap().contains("String formatName"));

            let fetch_user_data_function = symbols.iter()
                .find(|s| s.name == "fetchUserData");
            assert!(fetch_user_data_function.is_some());
            let fetch_user_data_function = fetch_user_data_function.unwrap();
            assert!(fetch_user_data_function.signature.as_ref().unwrap().contains("Future<String>"));
            assert!(fetch_user_data_function.signature.as_ref().unwrap().contains("async"));

            let count_stream_function = symbols.iter()
                .find(|s| s.name == "countStream");
            assert!(count_stream_function.is_some());
            let count_stream_function = count_stream_function.unwrap();
            assert!(count_stream_function.signature.as_ref().unwrap().contains("Stream<int>"));

            // Should extract generic function
            let process_data_function = symbols.iter()
                .find(|s| s.name == "processData");
            assert!(process_data_function.is_some());
            let process_data_function = process_data_function.unwrap();
            assert!(process_data_function.signature.as_ref().unwrap().contains("<T extends Comparable<T>>"));

            // Should extract typedefs
            let string_callback_typedef = symbols.iter()
                .find(|s| s.name == "StringCallback");
            assert!(string_callback_typedef.is_some());
            let string_callback_typedef = string_callback_typedef.unwrap();
            assert!(string_callback_typedef.signature.as_ref().unwrap().contains("typedef"));

            let number_processor_typedef = symbols.iter()
                .find(|s| s.name == "NumberProcessor");
            assert!(number_processor_typedef.is_some());
            let number_processor_typedef = number_processor_typedef.unwrap();
            assert!(number_processor_typedef.signature.as_ref().unwrap().contains("typedef"));
            assert!(number_processor_typedef.signature.as_ref().unwrap().contains("<T extends num>"));
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

            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);

            // Should extract Flutter widget classes
            let my_home_page_class = symbols.iter()
                .find(|s| s.name == "MyHomePage");
            assert!(my_home_page_class.is_some());
            let my_home_page_class = my_home_page_class.unwrap();
            assert!(my_home_page_class.signature.as_ref().unwrap().contains("extends StatefulWidget"));

            let state_class = symbols.iter()
                .find(|s| s.name == "_MyHomePageState");
            assert!(state_class.is_some());
            let state_class = state_class.unwrap();
            assert!(state_class.signature.as_ref().unwrap().contains("extends State<MyHomePage>"));
            assert!(state_class.signature.as_ref().unwrap().contains("with TickerProviderStateMixin"));

            let custom_button_class = symbols.iter()
                .find(|s| s.name == "CustomButton");
            assert!(custom_button_class.is_some());
            let custom_button_class = custom_button_class.unwrap();
            assert!(custom_button_class.signature.as_ref().unwrap().contains("extends StatelessWidget"));

            // Should extract lifecycle methods
            let init_state_method = symbols.iter()
                .find(|s| s.name == "initState");
            assert!(init_state_method.is_some());
            let init_state_method = init_state_method.unwrap();
            assert!(init_state_method.signature.as_ref().unwrap().contains("@override"));

            let dispose_method = symbols.iter()
                .find(|s| s.name == "dispose");
            assert!(dispose_method.is_some());

            // Should extract build methods
            let build_methods: Vec<_> = symbols.iter()
                .filter(|s| s.name == "build")
                .collect();
            assert_eq!(build_methods.len(), 2); // One for each widget

            let home_page_build = build_methods.iter()
                .find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("Widget build")));
            assert!(home_page_build.is_some());

            // Should extract custom methods
            let increment_method = symbols.iter()
                .find(|s| s.name == "_incrementCounter");
            assert!(increment_method.is_some());
            let increment_method = increment_method.unwrap();
            assert_eq!(increment_method.visibility, Some(crate::extractors::base::Visibility::Private));

            // Should extract createState method
            let create_state_method = symbols.iter()
                .find(|s| s.name == "createState");
            assert!(create_state_method.is_some());
            let create_state_method = create_state_method.unwrap();
            assert!(create_state_method.signature.as_ref().unwrap().contains("@override"));

            // Should extract fields
            let title_field = symbols.iter()
                .find(|s| s.name == "title");
            assert!(title_field.is_some());
            let title_field = title_field.unwrap();
            assert!(title_field.signature.as_ref().unwrap().contains("final String title"));

            let counter_field = symbols.iter()
                .find(|s| s.name == "_counter");
            assert!(counter_field.is_some());
            let counter_field = counter_field.unwrap();
            assert_eq!(counter_field.visibility, Some(crate::extractors::base::Visibility::Private));

            let controller_field = symbols.iter()
                .find(|s| s.name == "_controller");
            assert!(controller_field.is_some());
            let controller_field = controller_field.unwrap();
            assert!(controller_field.signature.as_ref().unwrap().contains("late AnimationController"));
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

            let mut extractor = DartExtractor::new(
                "dart".to_string(),
                "test.dart".to_string(),
                code.to_string(),
            );

            let symbols = extractor.extract_symbols(&tree);
            let relationships = extractor.extract_relationships(&tree, &symbols);
            let types = extractor.infer_types(&symbols);

            // Should extract inheritance relationships
            assert!(relationships.len() > 0);

            let rectangle_inheritance = relationships.iter()
                .find(|r| r.kind == crate::extractors::base::RelationshipKind::Extends && {
                    let from_symbol = symbols.iter()
                        .find(|s| s.id == r.from_symbol_id);
                    from_symbol.map_or(false, |s| s.name == "Rectangle")
                });
            assert!(rectangle_inheritance.is_some());

            let circle_inheritance = relationships.iter()
                .find(|r| r.kind == crate::extractors::base::RelationshipKind::Extends && {
                    let from_symbol = symbols.iter()
                        .find(|s| s.id == r.from_symbol_id);
                    from_symbol.map_or(false, |s| s.name == "Circle")
                });
            assert!(circle_inheritance.is_some());

            // Should extract mixin relationships
            let mixin_relationship = relationships.iter()
                .find(|r| r.kind == crate::extractors::base::RelationshipKind::Uses && {
                    let from_symbol = symbols.iter()
                        .find(|s| s.id == r.from_symbol_id);
                    from_symbol.map_or(false, |s| s.name == "ColoredRectangle")
                });
            assert!(mixin_relationship.is_some());

            // Should infer types
            assert!(types.len() > 0);

            // Should identify generic types
            let container_class = symbols.iter()
                .find(|s| s.name == "Container");
            assert!(container_class.is_some());
            let container_class = container_class.unwrap();
            assert!(container_class.signature.as_ref().unwrap().contains("<T>"));

            let process_method = symbols.iter()
                .find(|s| s.name == "process");
            assert!(process_method.is_some());
            let process_method = process_method.unwrap();
            assert!(process_method.signature.as_ref().unwrap().contains("<R>"));

            // Should handle getter/setter pairs
            let value_getter = symbols.iter()
                .find(|s| s.name == "value" && s.signature.as_ref().map_or(false, |sig| sig.contains("get")));
            assert!(value_getter.is_some());

            let value_setter = symbols.iter()
                .find(|s| s.name == "value" && s.signature.as_ref().map_or(false, |sig| sig.contains("set")));
            assert!(value_setter.is_some());
        }
    }
}