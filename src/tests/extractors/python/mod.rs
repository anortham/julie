// Port of Miller's comprehensive Python extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

// Submodule declarations
pub mod assignments;
pub mod decorators;
pub mod extractor;
pub mod functions;
pub mod helpers;
pub mod identifiers;
pub mod imports;
pub mod relationships;
pub mod signatures;
pub mod types;

use crate::extractors::base::{RelationshipKind, SymbolKind};
use crate::extractors::python::PythonExtractor;
use tree_sitter::Tree;

#[cfg(test)]
mod python_extractor_tests {
    use super::*;

    // Helper function to create a PythonExtractor and parse Python code
    pub(crate) fn create_extractor_and_parse(code: &str) -> (PythonExtractor, Tree) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let extractor = PythonExtractor::new("test.py".to_string(), code.to_string());
        (extractor, tree)
    }

    #[test]
    fn test_extract_basic_class_definitions() {
        let python_code = r#"
class User:
    """A user class for managing user data."""
    pass

class Admin(User):
    def __init__(self):
        super().__init__()
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let user_class = symbols.iter().find(|s| s.name == "User");
        assert!(user_class.is_some());
        let user_class = user_class.unwrap();
        assert_eq!(user_class.kind, SymbolKind::Class);
        assert!(user_class
            .signature
            .as_ref()
            .unwrap()
            .contains("class User"));
        assert!(user_class
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("A user class for managing user data"));

        let admin_class = symbols.iter().find(|s| s.name == "Admin");
        assert!(admin_class.is_some());
        let admin_class = admin_class.unwrap();
        assert_eq!(admin_class.kind, SymbolKind::Class);
        assert!(admin_class
            .signature
            .as_ref()
            .unwrap()
            .contains("class Admin extends User"));
    }

    #[test]
    fn test_extract_classes_with_decorators() {
        let python_code = r#"
@dataclass
@final
class Product:
    name: str
    price: float
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let product_class = symbols.iter().find(|s| s.name == "Product");
        assert!(product_class.is_some());
        assert!(product_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@dataclass @final class Product"));
    }

    #[test]
    fn test_extract_function_definitions_with_type_hints() {
        let python_code = r#"
def calculate_tax(amount: float, rate: float = 0.1) -> float:
    """Calculate tax amount."""
    return amount * rate

async def fetch_data(url: str) -> dict:
    """Async function to fetch data."""
    pass
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let calculate_tax = symbols.iter().find(|s| s.name == "calculate_tax");
        assert!(calculate_tax.is_some());
        let calculate_tax = calculate_tax.unwrap();
        assert_eq!(calculate_tax.kind, SymbolKind::Function);
        assert!(calculate_tax
            .signature
            .as_ref()
            .unwrap()
            .contains("def calculate_tax(amount: float, rate: float = 0.1): float"));
        assert!(calculate_tax
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Calculate tax amount."));

        let fetch_data = symbols.iter().find(|s| s.name == "fetch_data");
        assert!(fetch_data.is_some());
        let fetch_data = fetch_data.unwrap();
        assert_eq!(fetch_data.kind, SymbolKind::Function);
        assert!(fetch_data
            .signature
            .as_ref()
            .unwrap()
            .contains("async def fetch_data(url: str): dict"));
        assert!(fetch_data
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Async function to fetch data."));
    }

    #[test]
    fn test_extract_decorated_functions() {
        let python_code = r#"
@staticmethod
@cached
def get_config(key: str) -> str:
    return config[key]

@property
def full_name(self) -> str:
    return f"{self.first_name} {self.last_name}"
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let get_config = symbols.iter().find(|s| s.name == "get_config");
        assert!(get_config.is_some());
        assert!(get_config
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@staticmethod @cached def get_config"));

        let full_name = symbols.iter().find(|s| s.name == "full_name");
        assert!(full_name.is_some());
        assert!(full_name
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@property def full_name"));
    }

    #[test]
    fn test_extract_class_methods() {
        let python_code = r#"
class Calculator:
    def __init__(self, precision: int = 2):
        self.precision = precision

    def add(self, a: float, b: float) -> float:
        """Add two numbers."""
        return round(a + b, self.precision)

    def _internal_method(self):
        """Private method."""
        pass

    def __str__(self) -> str:
        return f"Calculator(precision={self.precision})"
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let calculator_class = symbols.iter().find(|s| s.name == "Calculator");
        assert!(calculator_class.is_some());

        let init_method = symbols.iter().find(|s| s.name == "__init__");
        assert!(init_method.is_some());
        let init_method = init_method.unwrap();
        assert_eq!(init_method.kind, SymbolKind::Constructor);
        assert_eq!(
            init_method.parent_id,
            Some(calculator_class.unwrap().id.clone())
        );

        let add_method = symbols.iter().find(|s| s.name == "add");
        assert!(add_method.is_some());
        let add_method = add_method.unwrap();
        assert_eq!(add_method.kind, SymbolKind::Method);
        assert!(add_method
            .signature
            .as_ref()
            .unwrap()
            .contains("def add(self, a: float, b: float): float"));
        assert!(add_method
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Add two numbers."));

        let internal_method = symbols.iter().find(|s| s.name == "_internal_method");
        assert!(internal_method.is_some());
        assert_eq!(
            internal_method.unwrap().visibility.as_ref().unwrap(),
            &crate::extractors::base::Visibility::Private
        );

        let str_method = symbols.iter().find(|s| s.name == "__str__");
        assert!(str_method.is_some());
        assert_eq!(
            str_method.unwrap().visibility.as_ref().unwrap(),
            &crate::extractors::base::Visibility::Public
        ); // Dunder methods are public
    }

    #[test]
    fn test_extract_variable_assignments_with_type_hints() {
        let python_code = r#"
# Module-level variables
API_URL: str = "https://api.example.com"
MAX_RETRIES = 3
is_debug: bool = False

class Config:
    def __init__(self):
        self.database_url: str = "postgresql://localhost"
        self.timeout = 30
        self._secret_key = "hidden"
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let api_url = symbols.iter().find(|s| s.name == "API_URL");
        assert!(api_url.is_some());
        let api_url = api_url.unwrap();
        assert_eq!(api_url.kind, SymbolKind::Constant);
        assert!(api_url
            .signature
            .as_ref()
            .unwrap()
            .contains(": str = \"https://api.example.com\""));

        let max_retries = symbols.iter().find(|s| s.name == "MAX_RETRIES");
        assert!(max_retries.is_some());
        assert_eq!(max_retries.unwrap().kind, SymbolKind::Constant); // Uppercase = constant

        let database_url = symbols.iter().find(|s| s.name == "database_url");
        assert!(database_url.is_some());
        let database_url = database_url.unwrap();
        assert_eq!(database_url.kind, SymbolKind::Property); // self.attribute = property
        assert!(database_url
            .signature
            .as_ref()
            .unwrap()
            .contains(": str = \"postgresql://localhost\""));

        let secret_key = symbols.iter().find(|s| s.name == "_secret_key");
        assert!(secret_key.is_some());
        assert_eq!(
            secret_key.unwrap().visibility.as_ref().unwrap(),
            &crate::extractors::base::Visibility::Private
        );
    }

    #[test]
    fn test_extract_multiple_assignments() {
        // Test multiple variable assignment: a, b = 1, 2
        let python_code = r#"
# Multiple assignment patterns
a, b = 1, 2
x, y, z = (10, 20, 30)
first, second = func_returning_tuple()

# Tuple unpacking
coords = (5.0, 10.0)
lat, lon = coords
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        // Should extract all variables from multiple assignments
        let a_var = symbols.iter().find(|s| s.name == "a");
        assert!(
            a_var.is_some(),
            "Should extract 'a' from multiple assignment"
        );
        assert_eq!(a_var.unwrap().kind, SymbolKind::Variable);

        let b_var = symbols.iter().find(|s| s.name == "b");
        assert!(
            b_var.is_some(),
            "Should extract 'b' from multiple assignment"
        );
        assert_eq!(b_var.unwrap().kind, SymbolKind::Variable);

        let x_var = symbols.iter().find(|s| s.name == "x");
        assert!(x_var.is_some(), "Should extract 'x' from tuple assignment");

        let y_var = symbols.iter().find(|s| s.name == "y");
        assert!(y_var.is_some(), "Should extract 'y' from tuple assignment");

        let z_var = symbols.iter().find(|s| s.name == "z");
        assert!(z_var.is_some(), "Should extract 'z' from tuple assignment");

        let first_var = symbols.iter().find(|s| s.name == "first");
        assert!(
            first_var.is_some(),
            "Should extract 'first' from function result"
        );

        let second_var = symbols.iter().find(|s| s.name == "second");
        assert!(
            second_var.is_some(),
            "Should extract 'second' from function result"
        );

        let lat_var = symbols.iter().find(|s| s.name == "lat");
        assert!(
            lat_var.is_some(),
            "Should extract 'lat' from tuple unpacking"
        );

        let lon_var = symbols.iter().find(|s| s.name == "lon");
        assert!(
            lon_var.is_some(),
            "Should extract 'lon' from tuple unpacking"
        );
    }

    #[test]
    fn test_extract_import_statements() {
        let python_code = r#"
import os
import json as js
from typing import List, Dict, Optional
from pathlib import Path
from .local_module import LocalClass as LC
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let os_import = symbols.iter().find(|s| s.name == "os");
        assert!(os_import.is_some());
        let os_import = os_import.unwrap();
        assert_eq!(os_import.kind, SymbolKind::Import);
        assert_eq!(os_import.signature.as_ref().unwrap(), "import os");

        let json_import = symbols.iter().find(|s| s.name == "js"); // Aliased
        assert!(json_import.is_some());
        assert_eq!(
            json_import.unwrap().signature.as_ref().unwrap(),
            "import json as js"
        );

        let list_import = symbols.iter().find(|s| s.name == "List");
        assert!(list_import.is_some());
        assert_eq!(
            list_import.unwrap().signature.as_ref().unwrap(),
            "from typing import List"
        );

        let local_import = symbols.iter().find(|s| s.name == "LC"); // Aliased
        assert!(local_import.is_some());
        assert_eq!(
            local_import.unwrap().signature.as_ref().unwrap(),
            "from .local_module import LocalClass as LC"
        );
    }

    #[test]
    fn test_extract_lambda_functions() {
        let python_code = r#"
numbers = [1, 2, 3, 4, 5]
squared = list(map(lambda x: x ** 2, numbers))
filtered = list(filter(lambda n: n > 3, numbers))
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let lambdas: Vec<_> = symbols
            .iter()
            .filter(|s| s.name.starts_with("<lambda:"))
            .collect();
        assert!(lambdas.len() >= 2);

        let square_lambda = lambdas
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("x ** 2"));
        assert!(square_lambda.is_some());
        assert_eq!(square_lambda.unwrap().kind, SymbolKind::Function);

        let filter_lambda = lambdas
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("n > 3"));
        assert!(filter_lambda.is_some());
    }

    #[test]
    fn test_extract_inheritance_relationships() {
        let python_code = r#"
class Animal:
    pass

class Dog(Animal):
    def bark(self):
        pass

class GoldenRetriever(Dog):
    def fetch(self):
        pass
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find inheritance relationships
        let dog_extends_animal = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .unwrap()
                    .name
                    == "Dog"
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .unwrap()
                    .name
                    == "Animal"
        });
        assert!(dog_extends_animal.is_some());

        let golden_extends_dog = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Extends
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .unwrap()
                    .name
                    == "GoldenRetriever"
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .unwrap()
                    .name
                    == "Dog"
        });
        assert!(golden_extends_dog.is_some());
    }

    #[test]
    fn test_extract_method_call_relationships() {
        let python_code = r#"
class DatabaseConnection:
    def connect(self) -> bool:
        return True

    def execute_query(self, query: str) -> list:
        return []

    def close(self) -> None:
        pass

class UserRepository:
    def __init__(self, db: DatabaseConnection):
        self.db = db

    def get_user(self, user_id: int):
        self.db.connect()
        try:
            result = self.db.execute_query(f"SELECT * FROM users WHERE id = {user_id}")
            return result[0] if result else None
        finally:
            self.db.close()
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let db_connection = symbols.iter().find(|s| s.name == "DatabaseConnection");
        let user_repo = symbols.iter().find(|s| s.name == "UserRepository");
        let connect_method = symbols.iter().find(|s| s.name == "connect");
        let get_user_method = symbols.iter().find(|s| s.name == "get_user");

        assert!(db_connection.is_some());
        assert!(user_repo.is_some());
        assert!(connect_method.is_some());
        assert!(get_user_method.is_some());

        // Check method call relationships
        let get_user_calls_connect = relationships.iter().find(|r| {
            r.kind == RelationshipKind::Calls
                && r.from_symbol_id == get_user_method.unwrap().id
                && r.to_symbol_id == connect_method.unwrap().id
        });
        assert!(get_user_calls_connect.is_some());
    }

    #[test]
    fn test_infer_types_from_annotations() {
        let python_code = r#"
def get_name() -> str:
    return "test"

def calculate(x: int, y: float) -> float:
    return x + y

username: str = "admin"
count: int = 42
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        let get_name = symbols.iter().find(|s| s.name == "get_name");
        assert!(get_name.is_some());
        assert_eq!(types.get(&get_name.unwrap().id).unwrap(), "str");

        let calculate = symbols.iter().find(|s| s.name == "calculate");
        assert!(calculate.is_some());
        assert_eq!(types.get(&calculate.unwrap().id).unwrap(), "float");

        let username = symbols.iter().find(|s| s.name == "username");
        assert!(username.is_some());
        assert_eq!(types.get(&username.unwrap().id).unwrap(), "str");
    }

    #[test]
    fn test_extract_modern_python_features() {
        let python_code = r#"
from typing import TypeVar, Generic, Protocol, Literal, Union, Final
from dataclasses import dataclass, field
from enum import Enum, auto

T = TypeVar('T', bound='Comparable')
API_VERSION: Final[str] = "v1.2.3"

class Color(Enum):
    """Color enumeration."""
    RED = auto()
    GREEN = auto()
    BLUE = auto()

class Comparable(Protocol):
    """Protocol for comparable objects."""
    def __lt__(self, other: 'Comparable') -> bool: ...

@dataclass(frozen=True, slots=True)
class Point:
    """Immutable point with slots."""
    x: float
    y: float
    metadata: dict = field(default_factory=dict)

class Container(Generic[T]):
    """Generic container class."""
    def __init__(self):
        self._items: list[T] = []
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        let t_var = symbols.iter().find(|s| s.name == "T");
        assert!(t_var.is_some());
        assert!(t_var
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("TypeVar('T', bound='Comparable')"));

        let api_version = symbols.iter().find(|s| s.name == "API_VERSION");
        assert!(api_version.is_some());
        assert!(api_version
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("Final[str] = \"v1.2.3\""));

        let color_enum = symbols.iter().find(|s| s.name == "Color");
        assert!(color_enum.is_some());
        let color_enum = color_enum.unwrap();
        assert_eq!(color_enum.kind, SymbolKind::Enum);
        assert!(color_enum
            .signature
            .as_ref()
            .unwrap()
            .contains("class Color extends Enum"));

        let red_value = symbols.iter().find(|s| s.name == "RED");
        assert!(red_value.is_some());
        assert_eq!(red_value.unwrap().kind, SymbolKind::EnumMember);

        let comparable = symbols.iter().find(|s| s.name == "Comparable");
        assert!(comparable.is_some());
        let comparable = comparable.unwrap();
        assert_eq!(comparable.kind, SymbolKind::Interface); // Protocol = Interface
        assert!(comparable
            .signature
            .as_ref()
            .unwrap()
            .contains("class Comparable extends Protocol"));

        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        let point = point.unwrap();
        assert!(point
            .signature
            .as_ref()
            .unwrap()
            .contains("@dataclass class Point"));
        assert!(point
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Immutable point with slots."));

        let container = symbols.iter().find(|s| s.name == "Container");
        assert!(container.is_some());
        assert!(container
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("class Container extends Generic[T]"));
    }

    #[test]
    fn test_comprehensive_python_code() {
        let python_code = r#"
from typing import List, Optional
from dataclasses import dataclass
import asyncio

@dataclass
class User:
    """A user data class."""
    id: int
    name: str
    email: Optional[str] = None

    @property
    def display_name(self) -> str:
        return self.name.title()

    @staticmethod
    def create_admin(name: str) -> 'User':
        return User(id=0, name=name, email=f"admin-{name}@example.com")

class UserManager:
    def __init__(self):
        self._users: List[User] = []
        self.MAX_USERS = 1000

    async def fetch_user(self, user_id: int) -> Optional[User]:
        """Fetch user by ID asynchronously."""
        await asyncio.sleep(0.1)  # Simulate async operation
        return next((u for u in self._users if u.id == user_id), None)

    def _validate_user(self, user: User) -> bool:
        """Private validation method."""
        return user.name and user.email

# Global configuration
DEBUG = True
users_cache = {}

def process_users(users: List[User]) -> List[str]:
    return [user.display_name for user in users if user.email]
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);
        let symbols = extractor.extract_symbols(&tree);

        // Check we extracted all major symbols
        assert!(symbols.iter().find(|s| s.name == "User").is_some());
        assert!(symbols.iter().find(|s| s.name == "UserManager").is_some());
        assert!(symbols.iter().find(|s| s.name == "display_name").is_some());
        assert!(symbols.iter().find(|s| s.name == "create_admin").is_some());
        assert!(symbols.iter().find(|s| s.name == "fetch_user").is_some());
        assert!(symbols
            .iter()
            .find(|s| s.name == "_validate_user")
            .is_some());
        assert!(symbols.iter().find(|s| s.name == "DEBUG").is_some());
        assert!(symbols.iter().find(|s| s.name == "process_users").is_some());

        // Check specific features
        let user_class = symbols.iter().find(|s| s.name == "User");
        assert!(user_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("@dataclass class User"));

        let fetch_user = symbols.iter().find(|s| s.name == "fetch_user");
        assert!(fetch_user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("async def fetch_user"));

        let validate_user = symbols.iter().find(|s| s.name == "_validate_user");
        assert_eq!(
            validate_user.unwrap().visibility.as_ref().unwrap(),
            &crate::extractors::base::Visibility::Private
        );

        let debug_var = symbols.iter().find(|s| s.name == "DEBUG");
        assert_eq!(debug_var.unwrap().kind, SymbolKind::Constant);
    }
}

// ========================================================================
// Identifier Extraction Tests (TDD RED phase)
// ========================================================================
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (call nodes)
// - Member access (attribute nodes)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust/C# extractor reference implementation pattern

#[cfg(test)]
mod identifier_extraction {
    use super::python_extractor_tests::create_extractor_and_parse;
    use crate::extractors::base::IdentifierKind;

    #[test]
    fn test_extract_function_calls() {
        let python_code = r#"
class Calculator:
    def add(self, a, b):
        return a + b

    def calculate(self):
        result = self.add(5, 3)      # Method call to add
        print(result)                 # Function call to print
        return result
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the function calls
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(
            add_call.is_some(),
            "Should extract 'add' method call identifier"
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
        let python_code = r#"
class User:
    def __init__(self, name, email):
        self.name = name
        self.email = email

    def print_info(self):
        username = self.name          # Member access: self.name
        user_email = self.email        # Member access: self.email
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);

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
        let python_code = r#"
class Service:
    def process(self):
        self.helper()              # Call to helper in same file

    def helper(self):
        # Helper method
        pass
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);

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
        let python_code = r#"
class DataService:
    def execute(self):
        result = user.account.balance   # Chained member access
        name = customer.profile.name     # Chained member access
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);

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
        let python_code = r#"
class Test:
    def run(self):
        self.process()
        self.process()  # Same call twice

    def process(self):
        pass
"#;

        let (mut extractor, tree) = create_extractor_and_parse(python_code);

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
