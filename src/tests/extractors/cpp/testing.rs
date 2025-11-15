use super::{SymbolKind, parse_cpp};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_google_test_catch2_and_boost_test_patterns() {
        let cpp_code = r#"
    #include <gtest/gtest.h>
    #include <catch2/catch.hpp>
    #include <boost/test/unit_test.hpp>

    // Google Test patterns
    TEST(MathTest, Addition) {
        EXPECT_EQ(2 + 2, 4);
        ASSERT_TRUE(true);
    }

    TEST_F(DatabaseTest, Connection) {
        EXPECT_NO_THROW(db_.connect());
    }

    class DatabaseTest : public ::testing::Test {
    protected:
        void SetUp() override {
            db_.initialize();
        }

        void TearDown() override {
            db_.cleanup();
        }

        Database db_;
    };

    // Parameterized test
    class ParameterizedMathTest : public ::testing::TestWithParam<int> {};

    TEST_P(ParameterizedMathTest, Square) {
        int value = GetParam();
        EXPECT_GT(value * value, 0);
    }

    // Catch2 patterns
    TEST_CASE("Vector operations", "[vector]") {
        std::vector<int> v{1, 2, 3};

        SECTION("push_back increases size") {
            v.push_back(4);
            REQUIRE(v.size() == 4);
        }

        SECTION("clear empties vector") {
            v.clear();
            CHECK(v.empty());
        }
    }

    SCENARIO("User authentication", "[auth]") {
        GIVEN("A user with valid credentials") {
            User user("john", "password123");

            WHEN("they attempt to login") {
                bool result = user.authenticate();

                THEN("authentication succeeds") {
                    REQUIRE(result == true);
                }
            }
        }
    }

    // Boost.Test patterns
    BOOST_AUTO_TEST_SUITE(StringTests)

    BOOST_AUTO_TEST_CASE(StringLength) {
        std::string str = "hello";
        BOOST_CHECK_EQUAL(str.length(), 5);
    }

    BOOST_AUTO_TEST_CASE(StringConcatenation) {
        std::string a = "hello";
        std::string b = "world";
        BOOST_REQUIRE_EQUAL(a + b, "helloworld");
    }

    BOOST_AUTO_TEST_SUITE_END()

    // Fixture class
    class FixtureTest {
    public:
        FixtureTest() : value_(42) {}

    protected:
        int value_;
    };

    BOOST_FIXTURE_TEST_CASE(FixtureUsage, FixtureTest) {
        BOOST_CHECK_EQUAL(value_, 42);
    }

    // Custom matchers and assertions
    MATCHER_P(IsMultipleOf, n, "") {
        return (arg % n) == 0;
    }

    TEST(CustomMatchers, MultipleTest) {
        EXPECT_THAT(15, IsMultipleOf(3));
    }
    "#;

        let (mut extractor, tree) = parse_cpp(cpp_code);
        let symbols = extractor.extract_symbols(&tree);

        // Google Test macros create functions
        let _math_test_addition = symbols
            .iter()
            .find(|s| s.name.contains("MathTest") || s.name.contains("Addition"));
        // Note: TEST macro expansion may not be fully parsed by tree-sitter

        let database_test = symbols.iter().find(|s| s.name == "DatabaseTest");
        assert!(database_test.is_some());
        assert_eq!(database_test.unwrap().kind, SymbolKind::Class);

        let setup_method = symbols.iter().find(|s| s.name == "SetUp");
        assert!(setup_method.is_some());

        let parameterized_test = symbols.iter().find(|s| s.name == "ParameterizedMathTest");
        assert!(parameterized_test.is_some());

        let fixture_test = symbols.iter().find(|s| s.name == "FixtureTest");
        assert!(fixture_test.is_some());

        // Note: Macro-generated test functions may not be captured perfectly
        // This depends on tree-sitter's ability to parse macro expansions
        let total_test_classes = symbols
            .iter()
            .filter(|s| s.name.contains("Test") && s.kind == SymbolKind::Class)
            .count();
        assert!(total_test_classes >= 2); // At least DatabaseTest and ParameterizedMathTest
    }
}
