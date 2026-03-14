//! Tests for the test quality metrics engine.
//!
//! TDD: These tests define the expected behavior for analyzing test function bodies
//! for assertion density, mock usage, and quality tiering.

#[cfg(test)]
mod tests {
    use crate::analysis::test_quality::analyze_test_body;

    // =========================================================================
    // Assertion counting — language-agnostic pattern matching
    // =========================================================================

    #[test]
    fn test_assertion_count_rust() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
            assert!(result > 0);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.assertion_count, 2, "Rust assert_eq! + assert! = 2");
    }

    #[test]
    fn test_assertion_count_python() {
        let body = r#"
            result = compute(42)
            self.assertEqual(result, 84)
            with pytest.raises(ValueError):
                compute(-1)
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 2,
            "Python self.assertEqual + pytest.raises = 2"
        );
    }

    #[test]
    fn test_assertion_count_javascript() {
        let body = r#"
            const result = compute(42);
            expect(result).toBe(84);
            expect(result).toEqual(84);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 2,
            "JS expect().toBe() + expect().toEqual() = 2"
        );
    }

    #[test]
    fn test_assertion_count_go() {
        let body = r#"
            result := Compute(42)
            require.Equal(t, 84, result)
            t.Fatal("should not reach here")
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 2,
            "Go require.Equal + t.Fatal = 2"
        );
    }

    #[test]
    fn test_assertion_count_java() {
        let body = r#"
            int result = compute(42);
            assertEquals(84, result);
            assertTrue(result > 0);
            assertNotNull(result);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 3,
            "Java assertEquals + assertTrue + assertNotNull = 3"
        );
    }

    #[test]
    fn test_assertion_count_csharp_fluent() {
        let body = r#"
            var result = Compute(42);
            result.Should().Be(84);
            Expect(result).To.BeGreaterThan(0);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 2,
            "C# Should + Expect( = 2"
        );
    }

    #[test]
    fn test_assertion_count_swift() {
        let body = r#"
            let result = compute(42)
            XCTAssertEqual(result, 84)
            XCTAssertTrue(result > 0)
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 2,
            "Swift XCTAssertEqual + XCTAssertTrue = 2"
        );
    }

    #[test]
    fn test_assertion_count_ruby() {
        let body = r#"
            result = compute(42)
            expect(result).to eq(84)
        "#;
        let metrics = analyze_test_body(body);
        // Ruby uses expect() chains — counted once via the expect( anchor pattern.
        // Chain methods (.to eq) are not separately counted to avoid double-counting.
        assert_eq!(
            metrics.assertion_count, 1,
            "Ruby expect().to eq = 1 (counted via expect( anchor)"
        );
    }

    #[test]
    fn test_assertion_count_php() {
        let body = r#"
            $result = compute(42);
            $this->assertEquals(84, $result);
            $this->assertTrue($result > 0);
        "#;
        let metrics = analyze_test_body(body);
        // PHP's assertEquals/assertTrue match the Java/JUnit assertion patterns.
        assert_eq!(
            metrics.assertion_count, 2,
            "PHP assertEquals + assertTrue = 2 (matched via JUnit patterns)"
        );
    }

    #[test]
    fn test_assertion_count_zero() {
        let body = r#"
            let x = 42;
            println!("hello");
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.assertion_count, 0, "No assertions in body");
    }

    // =========================================================================
    // Mock/stub counting
    // =========================================================================

    #[test]
    fn test_mock_count_basic() {
        let body = r#"
            let service = mock(UserService);
            let spy = jest.fn();
            service.get_user.returns(42);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.mock_count, 3,
            "mock + jest.fn( + spy = 3"
        );
    }

    #[test]
    fn test_mock_count_java_mockito() {
        let body = r#"
            @Mock
            private UserService service;
            @InjectMocks
            private UserController controller;
            Mockito.when(service.getUser(1)).thenReturn(user);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.mock_count, 3,
            "@Mock + @InjectMocks + Mockito = 3"
        );
    }

    #[test]
    fn test_mock_count_python_patch() {
        let body = r#"
            with patch('mymodule.UserService') as mock_service:
                mock_service.get_user.return_value = user
                result = controller.handle()
        "#;
        let metrics = analyze_test_body(body);
        // patch( + mock (in mock_service variable name won't match \bmock\b... let's check)
        // Actually "mock_service" starts with "mock" so \bmock\b won't match "mock_service"
        // because \b is a word boundary. The word "mock" in "mock_service" is followed by "_",
        // not a word boundary. Wait, underscore IS a word character. So \bmock\b won't match
        // "mock_service". It will only match standalone "mock".
        // But patch( matches.
        assert!(
            metrics.mock_count >= 1,
            "Python patch( should be detected"
        );
    }

    #[test]
    fn test_mock_count_zero() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.mock_count, 0, "No mocks in body");
    }

    #[test]
    fn test_mock_count_csharp_moq() {
        let body = r#"
            var mockService = new Moq.Mock<IUserService>();
            mockService.Setup(s => s.GetUser(1)).Returns(user);
        "#;
        let metrics = analyze_test_body(body);
        // Moq + mock (in mockService? no, "mockService" — \bmock\b won't match)
        // Actually "Moq" matches \bMoq\b. And "Mock" in "Moq.Mock" matches \bMock\b.
        assert!(
            metrics.mock_count >= 2,
            "C# Moq + Mock should be detected"
        );
    }

    // =========================================================================
    // Error testing detection
    // =========================================================================

    #[test]
    fn test_error_testing_rust() {
        let body = r#"
            let result = compute(-1);
            assert!(result.is_err());
            // should_err pattern
        "#;
        let metrics = analyze_test_body(body);
        // "should_err" matches error testing pattern
        assert!(
            metrics.has_error_testing,
            "Rust should_err should be detected"
        );
    }

    #[test]
    fn test_error_testing_python() {
        let body = r#"
            with pytest.raises(ValueError):
                compute(-1)
        "#;
        let metrics = analyze_test_body(body);
        assert!(
            metrics.has_error_testing,
            "Python pytest.raises should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_java() {
        let body = r#"
            assertThrows(IllegalArgumentException.class, () -> {
                compute(-1);
            });
        "#;
        let metrics = analyze_test_body(body);
        assert!(
            metrics.has_error_testing,
            "Java assertThrows should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_js_to_throw() {
        let body = r#"
            expect(() => compute(-1)).toThrow();
        "#;
        let metrics = analyze_test_body(body);
        assert!(
            metrics.has_error_testing,
            "JS .toThrow() should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_js_rejects() {
        let body = r#"
            await expect(computeAsync(-1)).rejects.toThrow();
        "#;
        let metrics = analyze_test_body(body);
        assert!(
            metrics.has_error_testing,
            "JS .rejects should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_none() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
        "#;
        let metrics = analyze_test_body(body);
        assert!(
            !metrics.has_error_testing,
            "No error testing patterns in body"
        );
    }

    // =========================================================================
    // Quality tier classification
    // =========================================================================

    #[test]
    fn test_quality_tier_stub() {
        let body = r#"
            // TODO: implement this test
            let x = 42;
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.assertion_count, 0);
        assert_eq!(metrics.quality_tier, "stub");
    }

    #[test]
    fn test_quality_tier_thin_single_assertion() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.assertion_count, 1);
        assert_eq!(metrics.quality_tier, "thin");
    }

    #[test]
    fn test_quality_tier_thin_low_density() {
        // 1 assertion in a very long body => assertion_density < 0.05
        let body = "let x = 1;\n".repeat(25) + "assert_eq!(x, 1);\n";
        let metrics = analyze_test_body(&body);
        assert_eq!(metrics.assertion_count, 1);
        assert_eq!(
            metrics.quality_tier, "thin",
            "1 assertion always produces thin tier"
        );
    }

    #[test]
    fn test_quality_tier_thin_low_density_multiple_assertions() {
        // 2 assertions in 100 non-empty lines => density = 0.02 < 0.05 => thin
        let mut lines: Vec<String> = (0..98)
            .map(|i| format!("    let x{} = {};", i, i))
            .collect();
        lines.push("    assert_eq!(x0, 0);".to_string());
        lines.push("    assert_eq!(x1, 1);".to_string());
        let body = lines.join("\n");

        let metrics = analyze_test_body(&body);
        assert_eq!(metrics.assertion_count, 2);
        assert_eq!(metrics.body_lines, 100);
        assert!(
            metrics.assertion_density < 0.05,
            "density {} should be < 0.05",
            metrics.assertion_density
        );
        assert_eq!(
            metrics.quality_tier, "thin",
            "2 assertions in 100 lines (density 0.02) should be thin"
        );
    }

    #[test]
    fn test_quality_tier_thorough_many_assertions() {
        let body = r#"
            let a = compute(1);
            assert_eq!(a, 1);
            let b = compute(2);
            assert_eq!(b, 4);
            let c = compute(3);
            assert_eq!(c, 9);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.assertion_count, 3);
        assert_eq!(metrics.quality_tier, "thorough");
    }

    #[test]
    fn test_quality_tier_thorough_error_testing() {
        let body = r#"
            let result = compute(-1);
            assert!(result.is_err());
            // should_err
        "#;
        let metrics = analyze_test_body(body);
        assert!(metrics.has_error_testing);
        assert_eq!(metrics.quality_tier, "thorough");
    }

    #[test]
    fn test_quality_tier_thorough_mocks_and_assertions() {
        let body = r#"
            let service = mock(UserService);
            let result = service.compute(42);
            assert_eq!(result, 84);
            assert!(result > 0);
        "#;
        let metrics = analyze_test_body(body);
        assert!(metrics.mock_count > 0);
        assert!(metrics.assertion_count >= 2);
        assert_eq!(metrics.quality_tier, "thorough");
    }

    #[test]
    fn test_quality_tier_adequate() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
            assert!(result > 0);
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(metrics.assertion_count, 2);
        assert_eq!(metrics.mock_count, 0);
        assert!(!metrics.has_error_testing);
        assert_eq!(metrics.quality_tier, "adequate");
    }

    // =========================================================================
    // Assertion density
    // =========================================================================

    #[test]
    fn test_assertion_density_calculation() {
        // 3 assertions in 20 lines => density = 0.15
        let mut lines = Vec::new();
        for i in 0..17 {
            lines.push(format!("    let x{} = {};", i, i));
        }
        lines.push("    assert_eq!(x0, 0);".to_string());
        lines.push("    assert_eq!(x1, 1);".to_string());
        lines.push("    assert_eq!(x2, 2);".to_string());
        let body = lines.join("\n");

        let metrics = analyze_test_body(&body);
        assert_eq!(metrics.assertion_count, 3);
        assert_eq!(metrics.body_lines, 20);
        let expected_density = 3.0 / 20.0;
        assert!(
            (metrics.assertion_density - expected_density).abs() < 0.001,
            "Expected density ~{}, got {}",
            expected_density,
            metrics.assertion_density
        );
    }

    #[test]
    fn test_assertion_density_empty_body() {
        let metrics = analyze_test_body("");
        assert_eq!(metrics.assertion_density, 0.0, "Empty body => density 0.0");
        assert_eq!(metrics.body_lines, 0);
    }

    // =========================================================================
    // Empty/None body handling
    // =========================================================================

    #[test]
    fn test_empty_body_produces_stub() {
        let metrics = analyze_test_body("");
        assert_eq!(metrics.assertion_count, 0);
        assert_eq!(metrics.mock_count, 0);
        assert_eq!(metrics.body_lines, 0);
        assert!(!metrics.has_error_testing);
        assert_eq!(metrics.quality_tier, "stub");
    }

    #[test]
    fn test_whitespace_only_body_produces_stub() {
        let metrics = analyze_test_body("   \n  \n   ");
        assert_eq!(metrics.quality_tier, "stub");
        assert_eq!(metrics.assertion_count, 0);
    }

    // =========================================================================
    // Pipeline integration: compute_test_quality_metrics on a real database
    // =========================================================================

    #[test]
    fn test_pipeline_integration_updates_metadata() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;

        // Create an in-memory database with the full schema
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Insert a fake file first (foreign key constraint)
        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_file.rs", "rust", "abc123", 100, 0],
            )
            .unwrap();

        // Insert a test symbol with is_test metadata and a code body
        let code_body = r#"fn test_something() {
    let result = compute(42);
    assert_eq!(result, 84);
    assert!(result > 0);
}"#;
        let metadata = r#"{"is_test":true}"#;
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-test-1",
                    "test_something",
                    "function",
                    "rust",
                    "test_file.rs",
                    code_body,
                    metadata,
                ],
            )
            .unwrap();

        // Insert a non-test symbol (should NOT be analyzed)
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-regular-1",
                    "compute",
                    "function",
                    "rust",
                    "test_file.rs",
                    "fn compute(x: i32) -> i32 { x * 2 }",
                    "{}",
                ],
            )
            .unwrap();

        // Run the pipeline function
        let stats = compute_test_quality_metrics(&db).unwrap();

        // Verify stats
        assert_eq!(stats.total_tests, 1, "Should have analyzed 1 test symbol");
        assert_eq!(stats.adequate, 1, "2 assertions, no mocks, no error testing = adequate");

        // Verify metadata was updated on the test symbol
        let updated_metadata: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-test-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let meta: serde_json::Value = serde_json::from_str(&updated_metadata).unwrap();
        assert!(meta["is_test"].as_bool().unwrap(), "is_test should still be true");
        assert!(meta["test_quality"].is_object(), "test_quality should be added");
        assert_eq!(meta["test_quality"]["assertion_count"].as_u64().unwrap(), 2);
        assert_eq!(meta["test_quality"]["quality_tier"].as_str().unwrap(), "adequate");

        // Verify non-test symbol was NOT modified
        let non_test_metadata: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-regular-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let non_test_meta: serde_json::Value = serde_json::from_str(&non_test_metadata).unwrap();
        assert!(
            non_test_meta.get("test_quality").is_none(),
            "Non-test symbol should not have test_quality"
        );
    }

    #[test]
    fn test_pipeline_integration_no_body() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_file.rs", "rust", "abc123", 100, 0],
            )
            .unwrap();

        // Test symbol with no code_context (NULL)
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, 0.0)",
                rusqlite::params![
                    "sym-test-no-body",
                    "test_empty",
                    "function",
                    "rust",
                    "test_file.rs",
                    r#"{"is_test":true}"#,
                ],
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&db).unwrap();
        assert_eq!(stats.total_tests, 1);
        assert_eq!(stats.no_body, 1, "Symbol with NULL code_context should be counted as no_body");
        assert_eq!(stats.stub, 1, "No body means stub tier");
    }

    #[test]
    fn test_pipeline_integration_preserves_existing_metadata() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_file.rs", "rust", "abc123", 100, 0],
            )
            .unwrap();

        // Test symbol with extra metadata that should be preserved
        let metadata = r#"{"is_test":true,"custom_flag":"keep_me"}"#;
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-test-preserve",
                    "test_preserve",
                    "function",
                    "rust",
                    "test_file.rs",
                    "assert_eq!(1, 1);",
                    metadata,
                ],
            )
            .unwrap();

        compute_test_quality_metrics(&db).unwrap();

        let updated: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-test-preserve'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let meta: serde_json::Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(
            meta["custom_flag"].as_str().unwrap(),
            "keep_me",
            "Existing metadata should be preserved"
        );
        assert!(meta["test_quality"].is_object(), "test_quality should be added");
    }

    // =========================================================================
    // Edge cases: pattern overlap and word boundaries
    // =========================================================================

    #[test]
    fn test_assert_in_variable_name_not_counted() {
        // "assertion_helper" contains "assert" but \bassert\b should only match whole word
        let body = r#"
            let assertion_helper = setup();
            let assertive = true;
        "#;
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 0,
            "assert in variable names should not match"
        );
    }

    #[test]
    fn test_multiple_assertions_on_same_line() {
        // Each pattern match counts independently
        let body = "assert_eq!(a, b); assert_ne!(c, d);";
        let metrics = analyze_test_body(body);
        assert_eq!(
            metrics.assertion_count, 2,
            "Two assertions on same line should both count"
        );
    }
}
