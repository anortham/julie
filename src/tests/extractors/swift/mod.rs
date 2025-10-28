// Port of Miller's comprehensive Swift extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

use crate::extractors::base::{RelationshipKind, SymbolKind, Visibility};
use crate::extractors::swift::SwiftExtractor;
use tree_sitter::Tree;

#[cfg(test)]
mod swift_extractor_tests {
    use super::*;

    // Helper function to create a SwiftExtractor and parse Swift code
    fn create_extractor_and_parse(code: &str) -> (SwiftExtractor, Tree) {
        use std::path::PathBuf;
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let extractor = SwiftExtractor::new(
            "swift".to_string(),
            "test.swift".to_string(),
            code.to_string(),
            &workspace_root,
        );
        (extractor, tree)
    }

    mod class_and_struct_extraction {
        use super::*;

        #[test]
        fn test_extract_classes_structs_and_their_members() {
            let swift_code = r#"
class Vehicle {
    var speed: Int = 0
    private let maxSpeed: Int

    init(maxSpeed: Int) {
        self.maxSpeed = maxSpeed
    }

    func accelerate() {
        speed += 1
    }

    deinit {
        print("Vehicle deallocated")
    }
}

struct Point {
    let x: Double
    let y: Double

    mutating func move(dx: Double, dy: Double) {
        x += dx
        y += dy
    }
}

public class Car: Vehicle {
    override func accelerate() {
        speed += 2
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            // Class extraction
            let vehicle = symbols.iter().find(|s| s.name == "Vehicle");
            assert!(vehicle.is_some());
            assert_eq!(vehicle.unwrap().kind, SymbolKind::Class);
            assert!(vehicle
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("class Vehicle"));

            // Properties
            let speed = symbols.iter().find(|s| s.name == "speed");
            assert!(speed.is_some());
            assert_eq!(speed.unwrap().kind, SymbolKind::Property);
            assert!(speed
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("var speed: Int"));

            let max_speed = symbols.iter().find(|s| s.name == "maxSpeed");
            assert!(max_speed.is_some());
            assert_eq!(max_speed.unwrap().visibility, Some(Visibility::Private));
            assert!(max_speed
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("let maxSpeed: Int"));

            // Methods
            let accelerate = symbols.iter().find(|s| s.name == "accelerate");
            assert!(accelerate.is_some());
            assert_eq!(accelerate.unwrap().kind, SymbolKind::Method);

            // Initializer
            let initializer = symbols.iter().find(|s| s.name == "init");
            assert!(initializer.is_some());
            assert_eq!(initializer.unwrap().kind, SymbolKind::Constructor);
            assert!(initializer
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("init(maxSpeed: Int)"));

            // Deinitializer
            let deinitializer = symbols.iter().find(|s| s.name == "deinit");
            assert!(deinitializer.is_some());
            assert_eq!(deinitializer.unwrap().kind, SymbolKind::Destructor);

            // Struct extraction
            let point = symbols.iter().find(|s| s.name == "Point");
            assert!(point.is_some());
            assert_eq!(point.unwrap().kind, SymbolKind::Struct);

            // Mutating method
            let move_func = symbols.iter().find(|s| s.name == "move");
            assert!(move_func.is_some());
            assert!(move_func
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("mutating func move"));

            // Inheritance
            let car = symbols.iter().find(|s| s.name == "Car");
            assert!(car.is_some());
            assert_eq!(car.unwrap().visibility, Some(Visibility::Public));
            assert!(car
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Car: Vehicle"));

            // Override method
            let car_accelerate = symbols
                .iter()
                .find(|s| s.name == "accelerate" && s.parent_id == Some(car.unwrap().id.clone()));
            assert!(car_accelerate.is_some());
            assert!(car_accelerate
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("override"));
        }
    }

    mod protocol_and_extension_extraction {
        use super::*;

        #[test]
        fn test_extract_protocols_extensions_and_conformances() {
            let swift_code = r#"
protocol Drawable {
    func draw()
    var area: Double { get }
    static var defaultColor: String { get set }
}

protocol Named {
    var name: String { get }
}

class Circle: Drawable, Named {
    let radius: Double
    let name: String

    init(radius: Double, name: String) {
        self.radius = radius
        self.name = name
    }

    func draw() {
        print("Drawing circle")
    }

    var area: Double {
        return Double.pi * radius * radius
    }

    static var defaultColor: String = "blue"
}

extension Circle {
    convenience init(diameter: Double) {
        self.init(radius: diameter / 2.0, name: "Unnamed")
    }

    func circumference() -> Double {
        return 2.0 * Double.pi * radius
    }
}

extension String {
    func reversed() -> String {
        return String(self.reversed())
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            // Protocol extraction
            let drawable = symbols.iter().find(|s| s.name == "Drawable");
            assert!(drawable.is_some());
            assert_eq!(drawable.unwrap().kind, SymbolKind::Interface);

            // Protocol requirements
            let protocol_draw = symbols
                .iter()
                .find(|s| s.name == "draw" && s.parent_id == Some(drawable.unwrap().id.clone()));
            assert!(protocol_draw.is_some());
            assert_eq!(protocol_draw.unwrap().kind, SymbolKind::Method);

            let protocol_area = symbols
                .iter()
                .find(|s| s.name == "area" && s.parent_id == Some(drawable.unwrap().id.clone()));
            assert!(protocol_area.is_some());
            assert!(protocol_area
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("{ get }"));

            let default_color = symbols.iter().find(|s| s.name == "defaultColor");
            assert!(default_color.is_some());
            assert!(default_color
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("static var"));
            assert!(default_color
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("{ get set }"));

            // Multiple protocol conformance
            let circle = symbols.iter().find(|s| s.name == "Circle");
            assert!(circle.is_some());
            assert!(circle
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Drawable, Named"));

            // Extension extraction
            let circle_extension = symbols.iter().find(|s| {
                s.name == "Circle" && s.signature.as_ref().unwrap().contains("extension")
            });
            assert!(circle_extension.is_some());

            // Extension methods
            let convenience = symbols.iter().find(|s| {
                s.name == "init" && s.signature.as_ref().unwrap().contains("convenience")
            });
            assert!(convenience.is_some());
            assert!(convenience
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("convenience init"));

            let circumference = symbols.iter().find(|s| s.name == "circumference");
            assert!(circumference.is_some());
        }
    }

    mod enum_and_associated_values {
        use super::*;

        #[test]
        fn test_extract_enums_with_cases_and_associated_values() {
            let swift_code = r#"
enum Direction {
    case north
    case south
    case east
    case west
}

enum Result<T> {
    case success(T)
    case failure(Error)
    case pending
}

indirect enum Expression {
    case number(Int)
    case addition(Expression, Expression)
    case multiplication(Expression, Expression)
}

enum HTTPStatusCode: Int, CaseIterable {
    case ok = 200
    case notFound = 404
    case internalServerError = 500

    var description: String {
        switch self {
        case .ok: return "OK"
        case .notFound: return "Not Found"
        case .internalServerError: return "Internal Server Error"
        }
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            // Simple enum
            let direction = symbols.iter().find(|s| s.name == "Direction");
            assert!(direction.is_some());
            assert_eq!(direction.unwrap().kind, SymbolKind::Enum);

            // Enum cases
            let north = symbols.iter().find(|s| s.name == "north");
            assert!(north.is_some());
            assert_eq!(north.unwrap().kind, SymbolKind::EnumMember);

            // Generic enum with associated values
            let result_symbol = symbols.iter().find(|s| s.name == "Result");
            assert!(result_symbol.is_some());
            assert!(result_symbol
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("enum Result<T>"));

            let success = symbols.iter().find(|s| s.name == "success");
            assert!(success.is_some());
            assert!(success
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("success(T)"));

            // Indirect enum
            let expression = symbols.iter().find(|s| s.name == "Expression");
            assert!(expression.is_some());
            assert!(expression
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("indirect enum"));

            // Enum with raw values and protocol conformance
            let http_status = symbols.iter().find(|s| s.name == "HTTPStatusCode");
            assert!(http_status.is_some());
            assert!(http_status
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(": Int, CaseIterable"));

            let ok = symbols.iter().find(|s| s.name == "ok");
            assert!(ok.is_some());
            assert!(ok.unwrap().signature.as_ref().unwrap().contains("= 200"));

            // Computed property in enum
            let description = symbols.iter().find(|s| {
                s.name == "description" && s.parent_id == Some(http_status.unwrap().id.clone())
            });
            assert!(description.is_some());
            assert!(description
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("var description: String"));
        }
    }

    mod generics_and_type_constraints {
        use super::*;

        #[test]
        fn test_extract_generic_types_and_functions_with_constraints() {
            let swift_code = r#"
struct Stack<Element> {
    private var items: [Element] = []

    mutating func push(_ item: Element) {
        items.append(item)
    }

    mutating func pop() -> Element? {
        return items.isEmpty ? nil : items.removeLast()
    }
}

func swapValues<T>(_ a: inout T, _ b: inout T) {
    let temp = a
    a = b
    b = temp
}

func findIndex<T: Equatable>(of valueToFind: T, in array: [T]) -> Int? {
    for (index, value) in array.enumerated() {
        if value == valueToFind {
            return index
        }
    }
    return nil
}

class Container<Item> where Item: Equatable {
    var items: [Item] = []

    func add(_ item: Item) {
        items.append(item)
    }

    func contains(_ item: Item) -> Bool {
        return items.contains(item)
    }
}

protocol Container {
    associatedtype Item
    var count: Int { get }
    mutating func append(_ item: Item)
    subscript(i: Int) -> Item { get }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            // Generic struct
            let stack = symbols.iter().find(|s| s.name == "Stack");
            assert!(stack.is_some());
            assert!(stack
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Stack<Element>"));

            // Generic function
            let swap_values = symbols.iter().find(|s| s.name == "swapValues");
            assert!(swap_values.is_some());
            assert!(swap_values
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("func swapValues<T>"));
            assert!(swap_values
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("inout T"));

            // Generic function with type constraint
            let find_index = symbols.iter().find(|s| s.name == "findIndex");
            assert!(find_index.is_some());
            assert!(find_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("<T: Equatable>"));

            // Generic class with where clause
            let container = symbols
                .iter()
                .find(|s| s.name == "Container" && s.kind == SymbolKind::Class);
            assert!(container.is_some());
            assert!(container
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("where Item: Equatable"));

            // Associated type in protocol
            let container_protocol = symbols
                .iter()
                .find(|s| s.name == "Container" && s.kind == SymbolKind::Interface);
            assert!(container_protocol.is_some());

            let associated_type = symbols
                .iter()
                .find(|s| s.name == "Item" && s.kind == SymbolKind::Type);
            assert!(associated_type.is_some());
            assert!(associated_type
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("associatedtype Item"));

            // Subscript
            let subscript_method = symbols.iter().find(|s| s.name == "subscript");
            assert!(subscript_method.is_some());
            assert!(subscript_method
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("subscript(i: Int) -> Item"));
        }
    }

    mod closures_and_function_types {
        use super::*;

        #[test]
        fn test_extract_closures_and_function_type_properties() {
            let swift_code = r#"
class EventHandler {
    var onComplete: (() -> Void)?
    var onSuccess: ((String) -> Void)?
    var onError: ((Error) -> Void)?
    var transformer: ((Int) -> String) = { number in
        return "Number: \(number)"
    }

    func processAsync(completion: @escaping (Result<String, Error>) -> Void) {
        // Async processing
    }

    lazy var expensiveComputation: () -> String = {
        return "Computed result"
    }()
}

func performOperation<T, U>(
    input: T,
    transform: (T) throws -> U,
    completion: @escaping (Result<U, Error>) -> Void
) {
    do {
        let result = try transform(input)
        completion(.success(result))
    } catch {
        completion(.failure(error))
    }
}

typealias CompletionHandler = (Bool, Error?) -> Void
typealias GenericHandler<T> = (T?, Error?) -> Void
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            // Function type properties
            let on_complete = symbols.iter().find(|s| s.name == "onComplete");
            assert!(on_complete.is_some());
            assert!(on_complete
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("(() -> Void)?"));

            let on_success = symbols.iter().find(|s| s.name == "onSuccess");
            assert!(on_success.is_some());
            assert!(on_success
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("((String) -> Void)?"));

            // Property with closure default value
            let transformer = symbols.iter().find(|s| s.name == "transformer");
            assert!(transformer.is_some());
            assert!(transformer
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("((Int) -> String)"));

            // Method with escaping closure
            let process_async = symbols.iter().find(|s| s.name == "processAsync");
            assert!(process_async.is_some());
            assert!(process_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@escaping"));
            assert!(process_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("(Result<String, Error>) -> Void"));

            // Lazy property
            let expensive_computation = symbols.iter().find(|s| s.name == "expensiveComputation");
            assert!(expensive_computation.is_some());
            assert!(expensive_computation
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("lazy var"));

            // Function with throwing closure
            let perform_operation = symbols.iter().find(|s| s.name == "performOperation");
            assert!(perform_operation.is_some());
            assert!(perform_operation
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("throws ->"));

            // Type aliases
            let completion_handler = symbols.iter().find(|s| s.name == "CompletionHandler");
            assert!(completion_handler.is_some());
            assert_eq!(completion_handler.unwrap().kind, SymbolKind::Type);
            assert!(completion_handler
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("typealias CompletionHandler"));

            let generic_handler = symbols.iter().find(|s| s.name == "GenericHandler");
            assert!(generic_handler.is_some());
            assert!(generic_handler
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("typealias GenericHandler<T>"));
        }
    }

    mod property_wrappers_and_attributes {
        use super::*;

        #[test]
        fn test_extract_property_wrappers_and_compiler_attributes() {
            let swift_code = r#"
@propertyWrapper
struct UserDefault<T> {
    let key: String
    let defaultValue: T

    var wrappedValue: T {
        get {
            UserDefaults.standard.object(forKey: key) as? T ?? defaultValue
        }
        set {
            UserDefaults.standard.set(newValue, forKey: key)
        }
    }
}

class SettingsManager {
    @UserDefault(key: "username", defaultValue: "")
    var username: String

    @UserDefault(key: "isFirstLaunch", defaultValue: true)
    var isFirstLaunch: Bool

    @Published var currentTheme: Theme = .light

    @objc dynamic var observableProperty: String = ""

    @available(iOS 13.0, *)
    func modernFunction() {
        // iOS 13+ only
    }

    @discardableResult
    func processData() -> Bool {
        return true
    }
}

@frozen
struct Point3D {
    let x: Double
    let y: Double
    let z: Double
}

@main
struct MyApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            // Property wrapper struct
            let user_default = symbols.iter().find(|s| s.name == "UserDefault");
            assert!(user_default.is_some());
            assert!(user_default
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@propertyWrapper"));

            // Property with wrapper
            let username = symbols.iter().find(|s| s.name == "username");
            assert!(username.is_some());
            assert!(username
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@UserDefault"));

            // Published property
            let current_theme = symbols.iter().find(|s| s.name == "currentTheme");
            assert!(current_theme.is_some());
            assert!(current_theme
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@Published"));

            // Objective-C interop
            let observable_property = symbols.iter().find(|s| s.name == "observableProperty");
            assert!(observable_property.is_some());
            assert!(observable_property
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@objc dynamic"));

            // Availability attribute
            let modern_function = symbols.iter().find(|s| s.name == "modernFunction");
            assert!(modern_function.is_some());
            assert!(modern_function
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@available(iOS 13.0, *)"));

            // Discardable result
            let process_data = symbols.iter().find(|s| s.name == "processData");
            assert!(process_data.is_some());
            assert!(process_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@discardableResult"));

            // Frozen struct
            let point3d = symbols.iter().find(|s| s.name == "Point3D");
            assert!(point3d.is_some());
            assert!(point3d
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@frozen"));

            // Main attribute
            let my_app = symbols.iter().find(|s| s.name == "MyApp");
            assert!(my_app.is_some());
            assert!(my_app
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("@main"));
        }
    }

    mod identifier_extraction {
        use super::*;
        use crate::extractors::base::IdentifierKind;

        #[test]
        fn test_extract_function_calls() {
            let swift_code = r#"
class Calculator {
    func add(_ a: Int, _ b: Int) -> Int {
        return a + b
    }

    func calculate() -> Int {
        let result = add(5, 3)      // Function call to add
        print(result)                // Function call to print
        return result
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);

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

            // Verify containing symbol is set correctly (should be inside calculate method)
            assert!(
                add_call.containing_symbol_id.is_some(),
                "Function call should have containing symbol"
            );

            // Find the calculate method symbol
            let calculate_method = symbols.iter().find(|s| s.name == "calculate").unwrap();

            // Verify the add call is contained within calculate method
            assert_eq!(
                add_call.containing_symbol_id.as_ref(),
                Some(&calculate_method.id),
                "add call should be contained within calculate method"
            );
        }

        #[test]
        fn test_extract_member_access() {
            let swift_code = r#"
class User {
    var name: String = ""
    var email: String = ""

    func printInfo() {
        print(self.name)       // Member access: self.name
        let email = self.email  // Member access: self.email
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);

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
        fn test_file_scoped_containing_symbol() {
            // This test ensures we ONLY match symbols from the SAME FILE
            // Critical bug fix from Rust implementation
            let swift_code = r#"
class Service {
    func process() {
        helper()              // Call to helper in same file
    }

    private func helper() {
        // Helper method
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Find the helper call
            let helper_call = identifiers.iter().find(|id| id.name == "helper");
            assert!(helper_call.is_some());
            let helper_call = helper_call.unwrap();

            // Verify it has a containing symbol (the process method)
            assert!(
                helper_call.containing_symbol_id.is_some(),
                "helper call should have containing symbol from same file"
            );

            // Verify the containing symbol is the process method
            let process_method = symbols.iter().find(|s| s.name == "process").unwrap();
            assert_eq!(
                helper_call.containing_symbol_id.as_ref(),
                Some(&process_method.id),
                "helper call should be contained within process method"
            );
        }

        #[test]
        fn test_chained_member_access() {
            let swift_code = r#"
class DataService {
    func execute() {
        let result = user.account.balance   // Chained member access
        let name = customer.profile.name     // Chained member access
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);

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
        fn test_no_duplicate_identifiers() {
            let swift_code = r#"
class Test {
    func run() {
        process()
        process()  // Same call twice
    }

    private func process() {
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);

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

    mod type_inference_and_relationships {
        use super::*;

        #[test]
        fn test_infer_types_from_swift_type_annotations_and_declarations() {
            let swift_code = r#"
class DataProcessor {
    func processString(_ input: String) -> String {
        return input.uppercased()
    }

    func processNumbers(_ numbers: [Int]) -> Double {
        return numbers.reduce(0, +) / Double(numbers.count)
    }

    var configuration: [String: Any] = [:]
    let processor: (String) -> String = { $0.lowercased() }
}

protocol DataSource {
    associatedtype Element
    func fetch() -> [Element]
}

class NetworkDataSource: DataSource {
    typealias Element = NetworkResponse

    func fetch() -> [NetworkResponse] {
        return []
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);
            let types = extractor.infer_types(&symbols);

            // Function return types
            let process_string = symbols.iter().find(|s| s.name == "processString");
            assert!(process_string.is_some());
            assert_eq!(
                types.get(&process_string.unwrap().id),
                Some(&"String".to_string())
            );

            let process_numbers = symbols.iter().find(|s| s.name == "processNumbers");
            assert!(process_numbers.is_some());
            assert_eq!(
                types.get(&process_numbers.unwrap().id),
                Some(&"Double".to_string())
            );

            // Property types
            let configuration = symbols.iter().find(|s| s.name == "configuration");
            assert!(configuration.is_some());
            assert_eq!(
                types.get(&configuration.unwrap().id),
                Some(&"[String: Any]".to_string())
            );

            let processor = symbols.iter().find(|s| s.name == "processor");
            assert!(processor.is_some());
            assert_eq!(
                types.get(&processor.unwrap().id),
                Some(&"(String) -> String".to_string())
            );
        }

        #[test]
        fn test_extract_inheritance_and_protocol_conformance_relationships() {
            let swift_code = r#"
protocol Vehicle {
    var speed: Double { get set }
    func start()
}

protocol Electric {
    var batteryLevel: Double { get }
}

class Car: Vehicle {
    var speed: Double = 0

    func start() {
        print("Car started")
    }
}

class Tesla: Car, Electric {
    var batteryLevel: Double = 100.0

    override func start() {
        super.start()
        print("Tesla started silently")
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);
            let relationships = extractor.extract_relationships(&tree, &symbols);

            // Should find inheritance and protocol conformance relationships
            assert!(relationships.len() >= 3);

            // Car implements Vehicle
            let car_vehicle = relationships.iter().find(|r| {
                r.kind == RelationshipKind::Implements
                    && symbols
                        .iter()
                        .find(|s| s.id == r.from_symbol_id)
                        .unwrap()
                        .name
                        == "Car"
                    && symbols
                        .iter()
                        .find(|s| s.id == r.to_symbol_id)
                        .unwrap()
                        .name
                        == "Vehicle"
            });
            assert!(car_vehicle.is_some());

            // Tesla extends Car
            let tesla_extends_car = relationships.iter().find(|r| {
                r.kind == RelationshipKind::Extends
                    && symbols
                        .iter()
                        .find(|s| s.id == r.from_symbol_id)
                        .unwrap()
                        .name
                        == "Tesla"
                    && symbols
                        .iter()
                        .find(|s| s.id == r.to_symbol_id)
                        .unwrap()
                        .name
                        == "Car"
            });
            assert!(tesla_extends_car.is_some());

            // Tesla implements Electric
            let tesla_electric = relationships.iter().find(|r| {
                r.kind == RelationshipKind::Implements
                    && symbols
                        .iter()
                        .find(|s| s.id == r.from_symbol_id)
                        .unwrap()
                        .name
                        == "Tesla"
                    && symbols
                        .iter()
                        .find(|s| s.id == r.to_symbol_id)
                        .unwrap()
                        .name
                        == "Electric"
            });
            assert!(tesla_electric.is_some());
        }
    }

    mod doc_comment_extraction {
        use super::*;

        #[test]
        fn test_extract_class_with_single_line_doc_comment() {
            let swift_code = r#"
/// UserService manages user authentication and login
class UserService {
    func authenticate() {}
}
"#;
            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            let user_service = symbols.iter().find(|s| s.name == "UserService");
            assert!(user_service.is_some(), "Should extract UserService class");
            let user_service = user_service.unwrap();
            assert_eq!(user_service.kind, SymbolKind::Class);
            assert!(
                user_service.doc_comment.is_some(),
                "Should have doc comment"
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
        fn test_extract_function_with_block_doc_comment() {
            let swift_code = r#"
/**
 * Validates user credentials
 * - Parameter username: The username to validate
 * - Returns: True if credentials are valid
 */
func validateCredentials(username: String) -> Bool {
    return true
}
"#;
            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            let validate_func = symbols.iter().find(|s| s.name == "validateCredentials");
            assert!(
                validate_func.is_some(),
                "Should extract validateCredentials function"
            );
            let validate_func = validate_func.unwrap();
            assert_eq!(validate_func.kind, SymbolKind::Function);
            assert!(
                validate_func.doc_comment.is_some(),
                "Should have doc comment"
            );
            let doc = validate_func.doc_comment.as_ref().unwrap();
            assert!(
                doc.contains("Validates user credentials"),
                "Doc should contain main description"
            );
            assert!(
                doc.contains("Parameter username"),
                "Doc should contain parameter documentation"
            );
        }

        #[test]
        fn test_extract_method_with_doc_comment() {
            let swift_code = r#"
class Server {
    /// Starts the server and begins listening for connections
    func start() {
        print("Server started")
    }

    /**
     * Stops the server gracefully
     * - Throws: ServerError if shutdown fails
     */
    func stop() throws {
        print("Server stopped")
    }
}
"#;
            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            let start_method = symbols.iter().find(|s| s.name == "start");
            assert!(start_method.is_some(), "Should extract start method");
            let start_method = start_method.unwrap();
            assert_eq!(start_method.kind, SymbolKind::Method);
            assert!(
                start_method.doc_comment.is_some(),
                "Should have doc comment"
            );
            assert!(
                start_method
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("Starts the server"),
                "Doc comment should be present"
            );

            let stop_method = symbols.iter().find(|s| s.name == "stop");
            assert!(stop_method.is_some(), "Should extract stop method");
            let stop_method = stop_method.unwrap();
            assert!(stop_method.doc_comment.is_some(), "Should have doc comment");
            assert!(
                stop_method
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("Stops the server"),
                "Doc comment should be present"
            );
        }

        #[test]
        fn test_extract_struct_with_doc_comment() {
            let swift_code = r#"
/// A point in 2D space
struct Point {
    let x: Double
    let y: Double
}
"#;
            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            let point = symbols.iter().find(|s| s.name == "Point");
            assert!(point.is_some(), "Should extract Point struct");
            let point = point.unwrap();
            assert_eq!(point.kind, SymbolKind::Struct);
            assert!(point.doc_comment.is_some(), "Should have doc comment");
            assert!(
                point
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("A point in 2D space"),
                "Doc comment should match"
            );
        }

        #[test]
        fn test_extract_protocol_with_doc_comment() {
            let swift_code = r#"
/// Protocol for database operations
protocol DatabaseProvider {
    /// Fetch records from database
    func fetch() -> [Record]
}
"#;
            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            let protocol = symbols.iter().find(|s| s.name == "DatabaseProvider");
            assert!(
                protocol.is_some(),
                "Should extract DatabaseProvider protocol"
            );
            let protocol = protocol.unwrap();
            assert_eq!(protocol.kind, SymbolKind::Interface);
            assert!(protocol.doc_comment.is_some(), "Should have doc comment");
            assert!(
                protocol
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("Protocol for database operations"),
                "Doc comment should be present"
            );
        }

        #[test]
        fn test_extract_enum_with_doc_comment() {
            let swift_code = r#"
/// HTTP status codes
enum HTTPStatus {
    case ok
    case notFound
}
"#;
            let (mut extractor, tree) = create_extractor_and_parse(swift_code);
            let symbols = extractor.extract_symbols(&tree);

            let http_status = symbols.iter().find(|s| s.name == "HTTPStatus");
            assert!(http_status.is_some(), "Should extract HTTPStatus enum");
            let http_status = http_status.unwrap();
            assert_eq!(http_status.kind, SymbolKind::Enum);
            assert!(http_status.doc_comment.is_some(), "Should have doc comment");
            assert!(
                http_status
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("HTTP status codes"),
                "Doc comment should be present"
            );
        }
    }
}
