//! Tests for the test quality metrics engine.
//!
//! Covers both the evidence-based assessment model (assess_test_quality)
//! and the regex fallback path (analyze_test_body), plus pipeline integration.

#[cfg(test)]
mod tests {
    use crate::analysis::test_quality::{
        EvidenceSource, TestQualityTier, analyze_test_body, assess_test_quality,
    };

    // =========================================================================
    // assess_test_quality: role-based short circuits
    // =========================================================================

    #[test]
    fn test_fixture_setup_is_not_applicable() {
        let assessment = assess_test_quality(
            Some("fixture_setup"),
            Some("do_setup();"),
            0,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::NotApplicable);
        assert_eq!(assessment.confidence, 1.0);
        assert_eq!(assessment.evidence.assertion_source, EvidenceSource::None);
    }

    #[test]
    fn test_teardown_is_not_applicable() {
        let assessment = assess_test_quality(
            Some("fixture_teardown"),
            Some("cleanup();"),
            0,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::NotApplicable);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_container_is_not_applicable() {
        let assessment = assess_test_quality(
            Some("test_container"),
            Some("describe('suite', () => { ... });"),
            0,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::NotApplicable);
        assert_eq!(assessment.confidence, 1.0);
    }

    // =========================================================================
    // assess_test_quality: stub detection (no body / placeholder body)
    // =========================================================================

    #[test]
    fn test_empty_body_is_stub() {
        let assessment = assess_test_quality(
            Some("test_case"),
            None, // no body
            0,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
        assert_eq!(assessment.evidence.assertion_source, EvidenceSource::None);
        assert_eq!(assessment.evidence.body_lines, 0);
    }

    #[test]
    fn test_placeholder_body_pass_is_stub() {
        let assessment = assess_test_quality(Some("test_case"), Some("pass"), 0, false, 0, false);
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_placeholder_body_todo_is_stub() {
        let assessment =
            assess_test_quality(Some("test_case"), Some("todo!()"), 0, false, 0, false);
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_placeholder_body_unimplemented_is_stub() {
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("unimplemented!()"),
            0,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_placeholder_body_ellipsis_is_stub() {
        let assessment = assess_test_quality(Some("test_case"), Some("..."), 0, false, 0, false);
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_placeholder_body_todo_comment_is_stub() {
        let assessment =
            assess_test_quality(Some("test_case"), Some("// TODO"), 0, false, 0, false);
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_placeholder_body_braces_with_pass_is_stub() {
        let assessment =
            assess_test_quality(Some("test_case"), Some("{ pass }"), 0, false, 0, false);
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    // =========================================================================
    // assess_test_quality: identifier-based evidence (high confidence)
    // =========================================================================

    #[test]
    fn test_identifier_thorough() {
        // 3 assertions + error testing from identifiers
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let x = compute();\nassert_eq!(x, 1);\nassert!(ok);\nassert_ne!(a, b);"),
            3,    // assertion_count
            true, // has_error_testing
            0,    // mock_count
            true, // has_identifier_evidence
        );
        assert_eq!(assessment.tier, TestQualityTier::Thorough);
        assert!(
            assessment.confidence >= 0.85,
            "confidence {} should be >= 0.85",
            assessment.confidence
        );
        assert_eq!(
            assessment.evidence.assertion_source,
            EvidenceSource::Identifier
        );
        assert_eq!(assessment.evidence.assertion_count, 3);
        assert!(assessment.evidence.has_error_testing);
    }

    #[test]
    fn test_identifier_adequate() {
        // 2 assertions, no error testing, no mocks
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let x = compute();\nassert_eq!(x, 1);\nassert!(ok);"),
            2,
            false,
            0,
            true,
        );
        assert_eq!(assessment.tier, TestQualityTier::Adequate);
        assert!(assessment.confidence >= 0.8);
        assert_eq!(
            assessment.evidence.assertion_source,
            EvidenceSource::Identifier
        );
    }

    #[test]
    fn test_identifier_thin() {
        // 1 assertion from identifiers
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let x = compute();\nassert_eq!(x, 1);"),
            1,
            false,
            0,
            true,
        );
        assert_eq!(assessment.tier, TestQualityTier::Thin);
        assert!(assessment.confidence >= 0.8);
        assert_eq!(
            assessment.evidence.assertion_source,
            EvidenceSource::Identifier
        );
    }

    #[test]
    fn test_identifier_stub() {
        // 0 assertions from identifiers (but we have identifier evidence)
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let x = compute();\nprintln!(\"done\");"),
            0,
            false,
            0,
            true,
        );
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 0.85);
        assert_eq!(
            assessment.evidence.assertion_source,
            EvidenceSource::Identifier
        );
    }

    #[test]
    fn test_identifier_thorough_with_mocks() {
        // 2 assertions + mocks from identifiers -> Thorough
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let mock = mock_service();\nassert_eq!(result, 42);\nassert!(ok);"),
            2,
            false,
            1,
            true,
        );
        assert_eq!(assessment.tier, TestQualityTier::Thorough);
        assert_eq!(assessment.evidence.mock_count, 1);
    }

    // =========================================================================
    // assess_test_quality: regex fallback (low confidence)
    // =========================================================================

    #[test]
    fn test_regex_zero_assertions_is_unknown() {
        // No identifier evidence, regex finds nothing -> Unknown, NOT Stub
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let x = compute(42);\nprintln!(\"result: {}\", x);"),
            0,     // regex found 0 assertions
            false, // no error testing
            0,     // no mocks
            false, // no identifier evidence
        );
        assert_eq!(assessment.tier, TestQualityTier::Unknown);
        assert_eq!(assessment.confidence, 0.3);
        assert_eq!(assessment.evidence.assertion_source, EvidenceSource::Regex);
    }

    #[test]
    fn test_regex_with_assertions_thorough() {
        // No identifier evidence, regex finds 3+ assertions -> Thorough at low confidence
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("assert_eq!(a, 1);\nassert_eq!(b, 2);\nassert_eq!(c, 3);"),
            3,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::Thorough);
        assert!(
            assessment.confidence <= 0.5,
            "regex confidence {} should be <= 0.5",
            assessment.confidence
        );
        assert_eq!(assessment.evidence.assertion_source, EvidenceSource::Regex);
    }

    #[test]
    fn test_regex_with_assertions_adequate() {
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("assert_eq!(a, 1);\nassert_eq!(b, 2);"),
            2,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::Adequate);
        assert_eq!(assessment.confidence, 0.4);
    }

    #[test]
    fn test_regex_with_assertions_thin() {
        let assessment = assess_test_quality(
            Some("test_case"),
            Some("let x = compute();\nassert_eq!(x, 42);"),
            1,
            false,
            0,
            false,
        );
        assert_eq!(assessment.tier, TestQualityTier::Thin);
        assert_eq!(assessment.confidence, 0.4);
    }

    // =========================================================================
    // analyze_test_body: regex-based analysis (backward compatibility)
    // =========================================================================

    #[test]
    fn test_assertion_count_rust() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
            assert!(result > 0);
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "Rust assert_eq! + assert! = 2"
        );
    }

    #[test]
    fn test_assertion_count_python() {
        let body = r#"
            result = compute(42)
            self.assertEqual(result, 84)
            with pytest.raises(ValueError):
                compute(-1)
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 3,
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "Swift XCTAssertEqual + XCTAssertTrue = 2"
        );
    }

    #[test]
    fn test_assertion_count_ruby() {
        let body = r#"
            result = compute(42)
            expect(result).to eq(84)
        "#;
        let assessment = analyze_test_body(body);
        // Ruby uses expect() chains, counted once via the expect( anchor pattern.
        assert_eq!(
            assessment.evidence.assertion_count, 1,
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
        let assessment = analyze_test_body(body);
        // PHP's assertEquals/assertTrue match the Java/JUnit assertion patterns.
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "PHP assertEquals + assertTrue = 2 (matched via JUnit patterns)"
        );
    }

    #[test]
    fn test_assertion_count_zero() {
        let body = r#"
            let x = 42;
            println!("hello");
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "No assertions in body"
        );
        // Regex path with 0 assertions => Unknown
        assert_eq!(assessment.tier, TestQualityTier::Unknown);
    }

    // =========================================================================
    // Mock/stub counting (via analyze_test_body regex path)
    // =========================================================================

    #[test]
    fn test_mock_count_basic() {
        let body = r#"
            let service = mock(UserService);
            let spy = jest.fn();
            service.get_user.returns(42);
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.mock_count, 3,
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.mock_count, 3,
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
        let assessment = analyze_test_body(body);
        assert!(
            assessment.evidence.mock_count >= 1,
            "Python patch( should be detected"
        );
    }

    #[test]
    fn test_mock_count_zero() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(assessment.evidence.mock_count, 0, "No mocks in body");
    }

    #[test]
    fn test_mock_count_csharp_moq() {
        let body = r#"
            var mockService = new Moq.Mock<IUserService>();
            mockService.Setup(s => s.GetUser(1)).Returns(user);
        "#;
        let assessment = analyze_test_body(body);
        // Moq matches \bMoq\b, Mock in "Moq.Mock" matches \bMock\b.
        assert!(
            assessment.evidence.mock_count >= 2,
            "C# Moq + Mock should be detected"
        );
    }

    // =========================================================================
    // Error testing detection (via analyze_test_body regex path)
    // =========================================================================

    #[test]
    fn test_error_testing_rust() {
        let body = r#"
            let result = compute(-1);
            assert!(result.is_err());
            result.should_err();
        "#;
        let assessment = analyze_test_body(body);
        assert!(
            assessment.evidence.has_error_testing,
            "Rust should_err should be detected"
        );
    }

    #[test]
    fn test_error_testing_python() {
        let body = r#"
            with pytest.raises(ValueError):
                compute(-1)
        "#;
        let assessment = analyze_test_body(body);
        assert!(
            assessment.evidence.has_error_testing,
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
        let assessment = analyze_test_body(body);
        assert!(
            assessment.evidence.has_error_testing,
            "Java assertThrows should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_js_to_throw() {
        let body = r#"
            expect(() => compute(-1)).toThrow();
        "#;
        let assessment = analyze_test_body(body);
        assert!(
            assessment.evidence.has_error_testing,
            "JS .toThrow() should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_js_rejects() {
        let body = r#"
            await expect(computeAsync(-1)).rejects.toThrow();
        "#;
        let assessment = analyze_test_body(body);
        assert!(
            assessment.evidence.has_error_testing,
            "JS .rejects should trigger error testing"
        );
    }

    #[test]
    fn test_error_testing_none() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
        "#;
        let assessment = analyze_test_body(body);
        assert!(
            !assessment.evidence.has_error_testing,
            "No error testing patterns in body"
        );
    }

    // =========================================================================
    // Quality tier classification (via analyze_test_body regex path)
    // =========================================================================

    #[test]
    fn test_quality_tier_zero_assertions_is_unknown() {
        // Changed from old behavior: regex with 0 assertions -> Unknown, not Stub
        let body = r#"
            // TODO: implement this test
            let x = 42;
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(assessment.evidence.assertion_count, 0);
        assert_eq!(assessment.tier, TestQualityTier::Unknown);
    }

    #[test]
    fn test_quality_tier_thin_single_assertion() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(assessment.evidence.assertion_count, 1);
        assert_eq!(assessment.tier, TestQualityTier::Thin);
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
        let assessment = analyze_test_body(body);
        assert_eq!(assessment.evidence.assertion_count, 3);
        assert_eq!(assessment.tier, TestQualityTier::Thorough);
    }

    #[test]
    fn test_quality_tier_thorough_error_testing() {
        let body = r#"
            let result = compute(-1);
            assert!(result.is_err());
            result.should_err();
        "#;
        let assessment = analyze_test_body(body);
        assert!(assessment.evidence.has_error_testing);
        assert_eq!(assessment.tier, TestQualityTier::Thorough);
    }

    #[test]
    fn test_quality_tier_thorough_mocks_and_assertions() {
        let body = r#"
            let service = mock(UserService);
            let result = service.compute(42);
            assert_eq!(result, 84);
            assert!(result > 0);
        "#;
        let assessment = analyze_test_body(body);
        assert!(assessment.evidence.mock_count > 0);
        assert!(assessment.evidence.assertion_count >= 2);
        assert_eq!(assessment.tier, TestQualityTier::Thorough);
    }

    #[test]
    fn test_quality_tier_adequate() {
        let body = r#"
            let result = compute(42);
            assert_eq!(result, 84);
            assert!(result > 0);
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(assessment.evidence.assertion_count, 2);
        assert_eq!(assessment.evidence.mock_count, 0);
        assert!(!assessment.evidence.has_error_testing);
        assert_eq!(assessment.tier, TestQualityTier::Adequate);
    }

    // =========================================================================
    // Empty/None body handling (via analyze_test_body)
    // =========================================================================

    #[test]
    fn test_empty_body_produces_stub_via_analyze() {
        let assessment = analyze_test_body("");
        assert_eq!(assessment.evidence.assertion_count, 0);
        assert_eq!(assessment.evidence.mock_count, 0);
        assert_eq!(assessment.evidence.body_lines, 0);
        assert!(!assessment.evidence.has_error_testing);
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.confidence, 1.0);
    }

    #[test]
    fn test_whitespace_only_body_produces_stub_via_analyze() {
        let assessment = analyze_test_body("   \n  \n   ");
        assert_eq!(assessment.tier, TestQualityTier::Stub);
        assert_eq!(assessment.evidence.assertion_count, 0);
    }

    // =========================================================================
    // Pipeline integration: compute_test_quality_metrics on a real database
    // =========================================================================

    #[test]
    fn test_pipeline_integration_updates_metadata() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        // Create an in-memory database with the full schema
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

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
        let stats = compute_test_quality_metrics(&db, &configs).unwrap();

        // Verify stats
        assert_eq!(stats.total_tests, 1, "Should have analyzed 1 test symbol");

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
        assert!(
            meta["is_test"].as_bool().unwrap(),
            "is_test should still be true"
        );
        assert!(
            meta["test_quality"].is_object(),
            "test_quality should be added"
        );
        assert_eq!(meta["test_quality"]["assertion_count"].as_u64().unwrap(), 2);
        // Regex fallback with 2 assertions -> adequate
        assert_eq!(
            meta["test_quality"]["quality_tier"].as_str().unwrap(),
            "adequate"
        );
        // Should have confidence field
        assert!(
            meta["test_quality"]["confidence"].as_f64().is_some(),
            "confidence should be present"
        );
        // Should have assertion_source
        assert_eq!(
            meta["test_quality"]["assertion_source"].as_str().unwrap(),
            "regex",
            "No identifiers inserted, so should be regex path"
        );

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
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

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

        let stats = compute_test_quality_metrics(&db, &configs).unwrap();
        assert_eq!(stats.total_tests, 1);
        assert_eq!(
            stats.no_body, 1,
            "Symbol with NULL code_context should be counted as no_body"
        );
        assert_eq!(stats.stub, 1, "No body means stub tier");
    }

    #[test]
    fn test_pipeline_integration_preserves_existing_metadata() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

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

        compute_test_quality_metrics(&db, &configs).unwrap();

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
        assert!(
            meta["test_quality"].is_object(),
            "test_quality should be added"
        );
    }

    #[test]
    fn test_pipeline_integration_with_identifier_evidence() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_file.rs", "rust", "abc123", 100, 0],
            )
            .unwrap();

        // Insert a test symbol
        let code_body =
            "fn test_with_identifiers() {\n    let x = compute();\n    assert_eq!(x, 42);\n}";
        let metadata = r#"{"is_test":true}"#;
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-test-ids",
                    "test_with_identifiers",
                    "function",
                    "rust",
                    "test_file.rs",
                    code_body,
                    metadata,
                ],
            )
            .unwrap();

        // Insert Call-kind identifiers for this test symbol
        // These simulate what the extractor would produce
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-1", "assert_eq", "call", "rust", "test_file.rs", 3, 4, 3, 20, "sym-test-ids",
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-2", "assert", "call", "rust", "test_file.rs", 4, 4, 4, 15, "sym-test-ids",
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-3", "assert_ne", "call", "rust", "test_file.rs", 5, 4, 5, 15, "sym-test-ids",
                ],
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&db, &configs).unwrap();
        assert_eq!(stats.total_tests, 1);

        let updated_metadata: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-test-ids'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&updated_metadata).unwrap();
        let tq = &meta["test_quality"];
        assert_eq!(
            tq["assertion_source"].as_str().unwrap(),
            "identifier",
            "Should use identifier evidence path"
        );
        assert!(
            tq["confidence"].as_f64().unwrap() >= 0.85,
            "Identifier path should have high confidence"
        );
        assert!(
            tq["assertion_count"].as_u64().unwrap() >= 2,
            "Should have counted identifier assertions"
        );
    }

    #[test]
    fn test_pipeline_integration_fixture_not_applicable() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_file.py", "python", "abc123", 100, 0],
            )
            .unwrap();

        // Insert a fixture_setup test symbol
        let metadata = r#"{"is_test":true,"test_role":"fixture_setup"}"#;
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-fixture",
                    "setUp",
                    "function",
                    "python",
                    "test_file.py",
                    "self.db = create_test_db()",
                    metadata,
                ],
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&db, &configs).unwrap();
        assert_eq!(stats.total_tests, 1);
        assert_eq!(stats.not_applicable, 1, "Fixture should be not_applicable");

        let updated: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-fixture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(
            meta["test_quality"]["quality_tier"].as_str().unwrap(),
            "n/a"
        );
        assert_eq!(meta["test_quality"]["confidence"].as_f64().unwrap(), 1.0);
    }

    // =========================================================================
    // Comment/string stripping -- false positive prevention
    // =========================================================================

    #[test]
    fn test_comment_assertions_not_counted() {
        let body = r#"
            let result = do_something();
            // assert_eq!(result, expected)  <-- commented out
            // should_err is a note
            println!("done");
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "commented-out assertions should not count"
        );
    }

    #[test]
    fn test_string_literal_mocks_not_counted() {
        let body = r#"
            let name = "mock_function_name";
            let desc = "when(something).thenReturn(value)";
            do_real_work();
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.mock_count, 0,
            "mock patterns inside strings should not count"
        );
    }

    #[test]
    fn test_block_comment_assertions_not_counted() {
        let body = r#"
            let x = 1;
            /* assert_eq!(x, 1);
               expect(x).toBe(1); */
            println!("test");
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "block-commented assertions should not count"
        );
    }

    #[test]
    fn test_real_assertions_still_counted_after_stripping() {
        let body = r#"
            // This test checks authentication
            let result = authenticate();
            assert_eq!(result, true);
            assert!(result.is_ok());
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "real assertions should still count after stripping comments"
        );
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
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "assert in variable names should not match"
        );
    }

    #[test]
    fn test_multiple_assertions_on_same_line() {
        // Each pattern match counts independently
        let body = "assert_eq!(a, b); assert_ne!(c, d);";
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "Two assertions on same line should both count"
        );
    }

    // =========================================================================
    // Identifier evidence: empty config falls back to regex
    // =========================================================================

    #[test]
    fn test_empty_evidence_config_falls_back_to_regex() {
        // A Go test has a LanguageConfig but its test_evidence has empty
        // assertion_identifiers. Even with call identifiers present, the
        // pipeline should fall back to the regex path. With no regex
        // assertions in the body either, the result should be Unknown
        // (not high-confidence Stub from the identifier path seeing 0 matches).
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

        // Verify Go has a config but empty assertion_identifiers
        let go_cfg = configs.get("go");
        assert!(go_cfg.is_some(), "Go should have a LanguageConfig");
        assert!(
            go_cfg
                .unwrap()
                .test_evidence
                .assertion_identifiers
                .is_empty(),
            "Go test_evidence.assertion_identifiers should be empty"
        );

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["handler_test.go", "go", "hash1", 200, 0],
            )
            .unwrap();

        // Go test with a body that has no regex-detectable assertions
        let code_body = "func TestHandler(t *testing.T) {\n    h := NewHandler()\n    h.Run()\n}";
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-go-test",
                    "TestHandler",
                    "function",
                    "go",
                    "handler_test.go",
                    code_body,
                    r#"{"is_test":true}"#,
                ],
            )
            .unwrap();

        // Insert call identifiers (simulating tree-sitter extraction)
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-go-1", "NewHandler", "call", "go", "handler_test.go", 2, 8, 2, 20, "sym-go-test",
                ],
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&db, &configs).unwrap();
        assert_eq!(stats.total_tests, 1);

        let updated: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-go-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&updated).unwrap();
        let tq = &meta["test_quality"];

        assert_eq!(
            tq["assertion_source"].as_str().unwrap(),
            "regex",
            "Go with empty evidence config should fall back to regex path"
        );
        assert_eq!(
            tq["quality_tier"].as_str().unwrap(),
            "unknown",
            "No regex assertions found => Unknown, not high-confidence Stub"
        );
    }

    #[test]
    fn test_identifier_evidence_without_matches_falls_back_to_regex_body() {
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_service.py", "python", "hash1", 200, 0],
            )
            .unwrap();

        let code_body = "def test_service():\n    helper()\n    assert result == expected";
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-python-test",
                    "test_service",
                    "function",
                    "python",
                    "test_service.py",
                    code_body,
                    r#"{"is_test":true,"test_role":"test_case"}"#,
                ],
            )
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-python-helper",
                    "helper",
                    "call",
                    "python",
                    "test_service.py",
                    2,
                    4,
                    2,
                    12,
                    "sym-python-test",
                ],
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&db, &configs).unwrap();
        assert_eq!(stats.total_tests, 1);

        let updated: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-python-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&updated).unwrap();
        let tq = &meta["test_quality"];

        assert_eq!(tq["assertion_source"].as_str().unwrap(), "regex");
        assert_eq!(tq["assertion_count"].as_u64().unwrap(), 1);
        assert_eq!(tq["quality_tier"].as_str().unwrap(), "thin");
        assert!((tq["confidence"].as_f64().unwrap() - 0.4).abs() < 0.001);
    }

    // =========================================================================
    // Identifier evidence: no substring matching
    // =========================================================================

    #[test]
    fn test_identifier_evidence_no_substring_matching() {
        // "assertion_report" should NOT match config entry "assert".
        // "mock_database" should NOT match config entry "mock".
        // This tests through the pipeline (compute_test_quality_metrics)
        // to verify that exact-match-only semantics hold end-to-end.
        use crate::analysis::test_quality::compute_test_quality_metrics;
        use crate::database::SymbolDatabase;
        use crate::search::LanguageConfigs;

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let configs = LanguageConfigs::load_embedded();

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["test_file.rs", "rust", "abc123", 100, 0],
            )
            .unwrap();

        // Test body that has no regex-detectable assertions either
        let code_body =
            "fn test_no_real_asserts() {\n    let r = assertion_report();\n    mock_database();\n}";
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, code_context, metadata, reference_score) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.0)",
                rusqlite::params![
                    "sym-substr",
                    "test_no_real_asserts",
                    "function",
                    "rust",
                    "test_file.rs",
                    code_body,
                    r#"{"is_test":true}"#,
                ],
            )
            .unwrap();

        // Insert identifiers that are substrings of config entries but not exact matches
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-sub-1", "assertion_report", "call", "rust", "test_file.rs", 2, 12, 2, 30, "sym-substr",
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    "id-sub-2", "mock_database", "call", "rust", "test_file.rs", 3, 4, 3, 18, "sym-substr",
                ],
            )
            .unwrap();

        let stats = compute_test_quality_metrics(&db, &configs).unwrap();
        assert_eq!(stats.total_tests, 1);

        let updated: String = db
            .conn
            .query_row(
                "SELECT metadata FROM symbols WHERE id = 'sym-substr'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&updated).unwrap();
        let tq = &meta["test_quality"];

        // With exact matching, "assertion_report" does NOT match "assert",
        // and "mock_database" does NOT match "mock". So identifier path
        // sees 0 assertions and 0 mocks.
        assert_eq!(
            tq["assertion_count"].as_u64().unwrap(),
            0,
            "assertion_report should not match config entry 'assert'"
        );
        assert_eq!(
            tq["mock_count"].as_u64().unwrap(),
            0,
            "mock_database should not match config entry 'mock'"
        );
    }

    // =========================================================================
    // TestQualityTier::as_str
    // =========================================================================

    #[test]
    fn test_tier_as_str() {
        assert_eq!(TestQualityTier::Thorough.as_str(), "thorough");
        assert_eq!(TestQualityTier::Adequate.as_str(), "adequate");
        assert_eq!(TestQualityTier::Thin.as_str(), "thin");
        assert_eq!(TestQualityTier::Stub.as_str(), "stub");
        assert_eq!(TestQualityTier::Unknown.as_str(), "unknown");
        assert_eq!(TestQualityTier::NotApplicable.as_str(), "n/a");
    }
}
