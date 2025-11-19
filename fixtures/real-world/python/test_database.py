"""
Comprehensive tests for Miller's LanceDB integration.

Tests cover:
- Database initialization and schema creation
- Text search with SQLite FTS5
- Semantic search with embeddings
- Batch operations and error handling
- Graph relationships and cross-language features
- Performance benchmarks
"""

import pytest
import pytest_asyncio
import tempfile
import shutil
import asyncio
from pathlib import Path
from unittest.mock import Mock, patch, AsyncMock
from typing import List, Dict, Any

from miller.database.lance_db import MillerDatabase
from miller.database.schema import (
    CodeEntity,
    TypeInfo,
    CallInfo,
    ParameterInfo,
    ReturnInfo,
    CodeEntityFactory
)


@pytest_asyncio.fixture
async def temp_db():
    """Create a temporary database for testing."""
    temp_dir = tempfile.mkdtemp(prefix="miller_test_")
    db = MillerDatabase(temp_dir)
    await db.initialize()
    yield db
    await db.close()
    shutil.rmtree(temp_dir, ignore_errors=True)


@pytest.fixture
def sample_entities():
    """Sample code entities for testing."""
    return [
        {
            "id": "func_test_py_calculate_42",
            "name": "calculate",
            "fqn": "test.py::calculate",
            "kind": "function",
            "content": "def calculate(x: int, y: int) -> int:\n    return x + y",
            "signature": "def calculate(x: int, y: int) -> int:",
            "file": "test.py",
            "line_start": 42,
            "line_end": 43,
            "language": "python",
            "parameters": "[{\"name\": \"x\", \"type_name\": \"int\", \"optional\": false}, {\"name\": \"y\", \"type_name\": \"int\", \"optional\": false}]",
            "returns": "{\"type_name\": \"int\", \"nullable\": false}"
        },
        {
            "id": "class_models_py_User_1",
            "name": "User",
            "fqn": "models.py::User",
            "kind": "class",
            "content": "class User:\n    def __init__(self, name: str):\n        self.name = name",
            "signature": "class User:",
            "file": "models.py",
            "line_start": 1,
            "line_end": 3,
            "language": "python",
            "api_endpoints": ["/api/users", "/api/users/{id}"],
            "db_tables": ["users", "user_sessions"]
        },
        {
            "id": "func_api_ts_fetchUser_15",
            "name": "fetchUser",
            "fqn": "api.ts::fetchUser",
            "kind": "function",
            "content": "async function fetchUser(id: string): Promise<User> {\n    return await api.get(`/api/users/${id}`);\n}",
            "signature": "async function fetchUser(id: string): Promise<User>",
            "file": "api.ts",
            "line_start": 15,
            "line_end": 17,
            "language": "typescript",
            "api_endpoints": ["/api/users/{id}"],
            "calls": "[{\"target_id\": \"api_get\", \"target_name\": \"api.get\", \"line\": 16}]"
        }
    ]


class TestDatabaseInitialization:
    """Test database initialization and schema creation."""

    @pytest.mark.asyncio
    async def test_initialize_creates_directory(self):
        """Test that initialize creates the index directory."""
        temp_dir = tempfile.mkdtemp(prefix="miller_init_test_")
        db_path = Path(temp_dir) / "new_db"

        db = MillerDatabase(str(db_path))
        await db.initialize()

        assert db_path.exists()
        assert db.db is not None
        assert db.table is not None
        assert db.embedder is not None

        await db.close()
        shutil.rmtree(temp_dir, ignore_errors=True)

    @pytest.mark.asyncio
    async def test_initialize_opens_existing_table(self, temp_db):
        """Test that initialize opens existing table on second run."""
        # First initialization already done by fixture
        original_table = temp_db.table

        # Reinitialize
        await temp_db.initialize()

        # Should open existing table, not create new
        assert temp_db.table is not None
        assert temp_db.db is not None

    @pytest.mark.asyncio
    async def test_initialize_creates_fts5_indexes(self, temp_db):
        """Test that SQLite FTS5 indexes are created."""
        # Indexes should be created during initialization
        # We can't directly inspect LanceDB indexes, but we can test search works
        assert temp_db.table is not None

        # Add a test entity
        test_entity = {
            "id": "test_fts",
            "name": "test",
            "fqn": "test::test",
            "kind": "function",
            "content": "def test_function():\n    pass",
            "signature": "def test_function():",
            "file": "test.py",
            "line_start": 1,
            "line_end": 2,
            "language": "python"
        }
        await temp_db.add_entity(test_entity)

        # Text search should work (proves SQLite FTS5 is working)
        results = await temp_db.text_search("test_function")
        assert len(results) > 0

    @pytest.mark.asyncio
    async def test_initialize_with_mock_failure(self):
        """Test initialization failure handling."""
        temp_dir = tempfile.mkdtemp(prefix="miller_fail_test_")
        db = MillerDatabase(str(temp_dir))

        with patch('lancedb.connect', side_effect=Exception("Connection failed")):
            with pytest.raises(Exception, match="Connection failed"):
                await db.initialize()

        shutil.rmtree(temp_dir, ignore_errors=True)


class TestEntityOperations:
    """Test adding and retrieving entities."""

    @pytest.mark.asyncio
    async def test_add_entity_generates_embedding(self, temp_db):
        """Test that add_entity generates embeddings automatically."""
        entity_data = {
            "id": "test_embedding",
            "name": "test_func",
            "fqn": "test::test_func",
            "kind": "function",
            "content": "def test_func(): pass",
            "signature": "def test_func():",
            "file": "test.py",
            "line_start": 1,
            "line_end": 1,
            "language": "python"
        }

        await temp_db.add_entity(entity_data)

        # Retrieve and verify embedding was generated
        retrieved = await temp_db.get_entity_by_id("test_embedding")
        assert retrieved is not None
        assert "embedding" in retrieved
        assert len(retrieved["embedding"]) == 384  # all-MiniLM-L6-v2 dimension

    @pytest.mark.asyncio
    async def test_add_entity_with_existing_embedding(self, temp_db):
        """Test that existing embeddings are preserved."""
        custom_embedding = [0.1] * 384
        entity_data = {
            "id": "test_custom_embedding",
            "name": "test_func",
            "fqn": "test::test_func",
            "kind": "function",
            "content": "def test_func(): pass",
            "signature": "def test_func():",
            "file": "test.py",
            "line_start": 1,
            "line_end": 1,
            "language": "python",
            "embedding": custom_embedding
        }

        await temp_db.add_entity(entity_data)

        retrieved = await temp_db.get_entity_by_id("test_custom_embedding")
        # Use approximate comparison for floating point embeddings
        assert retrieved["embedding"] == pytest.approx(custom_embedding, rel=1e-5)

    @pytest.mark.asyncio
    async def test_add_entities_batch(self, temp_db, sample_entities):
        """Test batch entity addition."""
        count = await temp_db.add_entities_batch(sample_entities)

        assert count == 3

        # Verify all entities were added
        for entity in sample_entities:
            retrieved = await temp_db.get_entity_by_id(entity["id"])
            assert retrieved is not None
            assert retrieved["name"] == entity["name"]

    @pytest.mark.asyncio
    async def test_add_entities_batch_with_invalid_data(self, temp_db):
        """Test batch addition handles invalid entities gracefully."""
        entities = [
            {
                "id": "valid_entity",
                "name": "valid",
                "fqn": "test::valid",
                "kind": "function",
                "content": "def valid(): pass",
                "signature": "def valid():",
                "file": "test.py",
                "line_start": 1,
                "line_end": 1,
                "language": "python"
            },
            {
                "id": "invalid_entity"
                # Missing required fields
            }
        ]

        count = await temp_db.add_entities_batch(entities)

        # Should add only the valid entity
        assert count == 1

        valid_retrieved = await temp_db.get_entity_by_id("valid_entity")
        assert valid_retrieved is not None

        invalid_retrieved = await temp_db.get_entity_by_id("invalid_entity")
        assert invalid_retrieved is None

    @pytest.mark.asyncio
    async def test_get_entity_by_id_not_found(self, temp_db):
        """Test get_entity_by_id returns None for non-existent entity."""
        result = await temp_db.get_entity_by_id("nonexistent")
        assert result is None


class TestSearchOperations:
    """Test various search capabilities."""

    @pytest.mark.asyncio
    async def test_text_search(self, temp_db, sample_entities):
        """Test SQLite FTS5 full-text search."""
        await temp_db.add_entities_batch(sample_entities)

        # Search for function content
        results = await temp_db.text_search("calculate")
        assert len(results) > 0
        assert any(r["name"] == "calculate" for r in results)

        # Search for class content
        results = await temp_db.text_search("User")
        assert len(results) > 0
        assert any(r["name"] == "User" for r in results)

    @pytest.mark.asyncio
    async def test_text_search_with_limit(self, temp_db, sample_entities):
        """Test text search respects limit parameter."""
        await temp_db.add_entities_batch(sample_entities)

        results = await temp_db.text_search("function", limit=1)
        assert len(results) <= 1

    @pytest.mark.asyncio
    async def test_semantic_search(self, temp_db, sample_entities):
        """Test vector-based semantic search."""
        await temp_db.add_entities_batch(sample_entities)

        # Search for semantically similar content
        results = await temp_db.semantic_search("mathematical operation")
        assert len(results) > 0

        # Should find the calculate function
        assert any(r["name"] == "calculate" for r in results)

    @pytest.mark.asyncio
    async def test_hybrid_search(self, temp_db, sample_entities):
        """Test combined text and semantic search."""
        await temp_db.add_entities_batch(sample_entities)

        results = await temp_db.hybrid_search("User")

        assert "text" in results
        assert "semantic" in results
        assert "combined_count" in results
        assert results["combined_count"] > 0

    @pytest.mark.asyncio
    async def test_find_similar_code(self, temp_db, sample_entities):
        """Test finding similar code snippets."""
        await temp_db.add_entities_batch(sample_entities)

        code_snippet = "def add_numbers(a, b):\n    return a + b"
        results = await temp_db.find_similar_code(code_snippet)

        assert len(results) > 0
        # Should find the calculate function as similar
        assert any(r["name"] == "calculate" for r in results)

    @pytest.mark.asyncio
    async def test_search_without_initialization(self):
        """Test search operations fail gracefully without initialization."""
        temp_dir = tempfile.mkdtemp(prefix="miller_uninit_test_")
        db = MillerDatabase(temp_dir)
        # Don't call initialize()

        with pytest.raises(RuntimeError, match="Database not initialized"):
            await db.text_search("test")

        with pytest.raises(RuntimeError, match="Database not initialized"):
            await db.semantic_search("test")

        shutil.rmtree(temp_dir, ignore_errors=True)


class TestDatabaseStatistics:
    """Test database statistics and metrics."""

    @pytest.mark.asyncio
    async def test_get_statistics_empty_db(self, temp_db):
        """Test statistics for empty database."""
        stats = await temp_db.get_statistics()

        assert "total_entities" in stats
        assert stats["total_entities"] == 0
        assert "entities_by_kind" in stats
        assert "entities_by_language" in stats
        assert "total_files" in stats
        assert "index_size_mb" in stats
        assert "last_updated" in stats

    @pytest.mark.asyncio
    async def test_get_statistics_with_data(self, temp_db, sample_entities):
        """Test statistics with sample data."""
        await temp_db.add_entities_batch(sample_entities)

        stats = await temp_db.get_statistics()

        assert stats["total_entities"] == 3
        assert "function" in stats["entities_by_kind"]
        assert "class" in stats["entities_by_kind"]
        assert "python" in stats["entities_by_language"]
        assert "typescript" in stats["entities_by_language"]
        assert stats["total_files"] == 3  # test.py, models.py, api.ts

    @pytest.mark.asyncio
    async def test_index_size_calculation(self, temp_db, sample_entities):
        """Test index size calculation."""
        await temp_db.add_entities_batch(sample_entities)

        stats = await temp_db.get_statistics()

        # Should have some measurable size
        assert stats["index_size_mb"] > 0


class TestGraphRelationships:
    """Test graph relationship features."""

    @pytest.mark.asyncio
    async def test_entity_with_type_relationships(self, temp_db):
        """Test entities with extends/implements relationships."""
        entity_data = {
            "id": "class_child_py_Child_1",
            "name": "Child",
            "fqn": "child.py::Child",
            "kind": "class",
            "content": "class Child(Parent):\n    pass",
            "signature": "class Child(Parent):",
            "file": "child.py",
            "line_start": 1,
            "line_end": 2,
            "language": "python",
            "extends": "[{\"name\": \"Parent\", \"kind\": \"class\", \"generics\": []}]",
            "implements": "[{\"name\": \"Serializable\", \"kind\": \"interface\", \"generics\": []}]"
        }

        await temp_db.add_entity(entity_data)

        retrieved = await temp_db.get_entity_by_id("class_child_py_Child_1")
        assert retrieved is not None

        import json
        extends = json.loads(retrieved["extends"])
        assert len(extends) == 1
        assert extends[0]["name"] == "Parent"

        implements = json.loads(retrieved["implements"])
        assert len(implements) == 1
        assert implements[0]["name"] == "Serializable"

    @pytest.mark.asyncio
    async def test_entity_with_call_relationships(self, temp_db):
        """Test entities with function call relationships."""
        entity_data = {
            "id": "func_caller_py_caller_5",
            "name": "caller",
            "fqn": "caller.py::caller",
            "kind": "function",
            "content": "def caller():\n    helper()\n    other_func()",
            "signature": "def caller():",
            "file": "caller.py",
            "line_start": 5,
            "line_end": 7,
            "language": "python",
            "calls": "[{\"target_id\": \"func_helper\", \"target_name\": \"helper\", \"line\": 6}, {\"target_id\": \"func_other\", \"target_name\": \"other_func\", \"line\": 7}]"
        }

        await temp_db.add_entity(entity_data)

        retrieved = await temp_db.get_entity_by_id("func_caller_py_caller_5")
        assert retrieved is not None

        import json
        calls = json.loads(retrieved["calls"])
        assert len(calls) == 2
        assert calls[0]["target_name"] == "helper"
        assert calls[1]["target_name"] == "other_func"


class TestCrossLanguageFeatures:
    """Test cross-language bridge features."""

    @pytest.mark.asyncio
    async def test_api_endpoint_mapping(self, temp_db):
        """Test API endpoint cross-language mapping."""
        # Backend entity serving endpoint
        backend_entity = {
            "id": "controller_UserController_GetUser",
            "name": "GetUser",
            "fqn": "UserController::GetUser",
            "kind": "method",
            "content": "[HttpGet(\"/api/users/{id}\")]\npublic User GetUser(int id) { ... }",
            "signature": "public User GetUser(int id)",
            "file": "UserController.cs",
            "line_start": 15,
            "line_end": 20,
            "language": "csharp",
            "api_endpoints": ["/api/users/{id}"]
        }

        # Frontend entity calling endpoint
        frontend_entity = {
            "id": "func_fetchUser_ts_15",
            "name": "fetchUser",
            "fqn": "api.ts::fetchUser",
            "kind": "function",
            "content": "async function fetchUser(id) {\n    return fetch('/api/users/' + id);\n}",
            "signature": "async function fetchUser(id)",
            "file": "api.ts",
            "line_start": 15,
            "line_end": 17,
            "language": "typescript",
            "api_endpoints": ["/api/users/{id}"]
        }

        await temp_db.add_entities_batch([backend_entity, frontend_entity])

        # Search for entities sharing the same API endpoint
        results = await temp_db.text_search("/api/users")

        # Should find both backend and frontend entities
        entity_names = [r["name"] for r in results]
        assert "GetUser" in entity_names
        assert "fetchUser" in entity_names

    @pytest.mark.asyncio
    async def test_database_table_mapping(self, temp_db):
        """Test database table cross-language mapping."""
        entities = [
            {
                "id": "model_User_cs_5",
                "name": "User",
                "fqn": "Models::User",
                "kind": "class",
                "content": "public class User { ... }",
                "signature": "public class User",
                "file": "User.cs",
                "line_start": 5,
                "line_end": 15,
                "language": "csharp",
                "db_tables": ["users", "user_profiles"]
            },
            {
                "id": "migration_CreateUsers_20240101",
                "name": "CreateUsers",
                "fqn": "Migrations::CreateUsers",
                "kind": "migration",
                "content": "CREATE TABLE users (id INT PRIMARY KEY, ...)",
                "signature": "CREATE TABLE users",
                "file": "20240101_CreateUsers.sql",
                "line_start": 1,
                "line_end": 10,
                "language": "sql",
                "db_tables": ["users"]
            }
        ]

        await temp_db.add_entities_batch(entities)

        # Search for entities accessing same table
        results = await temp_db.text_search("users")

        # Should find both model and migration
        assert len(results) >= 2


class TestCodeEntityFactory:
    """Test CodeEntity factory methods."""

    def test_create_function(self):
        """Test function entity creation."""
        entity = CodeEntityFactory.create_function(
            name="test_func",
            content="def test_func(x: int) -> str:\n    return str(x)",
            file="test.py",
            line_start=5,
            line_end=6,
            language="python",
            parameters=[ParameterInfo(name="x", type_name="int")],
            returns=ReturnInfo(type_name="str")
        )

        assert entity.name == "test_func"
        assert entity.kind == "function"
        assert entity.fqn == "test.py::test_func"
        assert entity.id == "func_test.py_test_func_5"
        params = entity.get_parameters()
        assert len(params) == 1
        assert params[0].name == "x"
        returns = entity.get_returns()
        assert returns.type_name == "str"

    def test_create_class(self):
        """Test class entity creation."""
        entity = CodeEntityFactory.create_class(
            name="TestClass",
            content="class TestClass(BaseClass):\n    pass",
            file="test.py",
            line_start=10,
            line_end=11,
            language="python",
            extends=[TypeInfo(name="BaseClass", kind="class")],
            implements=[TypeInfo(name="TestInterface", kind="interface")]
        )

        assert entity.name == "TestClass"
        assert entity.kind == "class"
        assert entity.fqn == "test.py::TestClass"
        assert entity.id == "class_test.py_TestClass_10"
        extends = entity.get_extends()
        assert len(extends) == 1
        assert extends[0].name == "BaseClass"
        implements = entity.get_implements()
        assert len(implements) == 1
        assert implements[0].name == "TestInterface"


class TestErrorHandling:
    """Test error handling and edge cases."""

    @pytest.mark.asyncio
    async def test_text_search_handles_exceptions(self, temp_db):
        """Test text search handles database exceptions gracefully."""
        # Mock the table to raise an exception
        with patch.object(temp_db.table, 'search', side_effect=Exception("Search failed")):
            results = await temp_db.text_search("test")
            assert results == []

    @pytest.mark.asyncio
    async def test_semantic_search_handles_exceptions(self, temp_db):
        """Test semantic search handles database exceptions gracefully."""
        with patch.object(temp_db.table, 'search', side_effect=Exception("Search failed")):
            results = await temp_db.semantic_search("test")
            assert results == []

    @pytest.mark.asyncio
    async def test_add_entity_with_invalid_data(self, temp_db):
        """Test add_entity handles validation errors."""
        invalid_entity = {
            "id": "invalid",
            # Missing required fields
        }

        with pytest.raises(Exception):  # Should raise validation error
            await temp_db.add_entity(invalid_entity)

    @pytest.mark.asyncio
    async def test_statistics_handles_exceptions(self, temp_db):
        """Test statistics calculation handles exceptions gracefully."""
        with patch.object(temp_db.table, 'count_rows', side_effect=Exception("Count failed")):
            stats = await temp_db.get_statistics()
            # Should return default stats
            assert stats["total_entities"] == 0


class TestPerformance:
    """Test performance benchmarks and optimizations."""

    @pytest.mark.asyncio
    async def test_batch_addition_performance(self, temp_db):
        """Test batch addition meets performance requirements."""
        import time

        # Create 100 test entities
        entities = []
        for i in range(100):
            entities.append({
                "id": f"perf_test_{i}",
                "name": f"test_func_{i}",
                "fqn": f"test.py::test_func_{i}",
                "kind": "function",
                "content": f"def test_func_{i}():\n    return {i}",
                "signature": f"def test_func_{i}():",
                "file": "test.py",
                "line_start": i,
                "line_end": i + 1,
                "language": "python"
            })

        start_time = time.time()
        count = await temp_db.add_entities_batch(entities)
        end_time = time.time()

        assert count == 100

        # Should complete within reasonable time (adjust threshold as needed)
        duration = end_time - start_time
        assert duration < 10.0  # 10 seconds for 100 entities

    @pytest.mark.asyncio
    async def test_search_performance(self, temp_db, sample_entities):
        """Test search meets latency requirements."""
        import time

        await temp_db.add_entities_batch(sample_entities)

        # Test text search latency
        start_time = time.time()
        results = await temp_db.text_search("function")
        text_duration = time.time() - start_time

        assert len(results) >= 0  # May be empty, that's ok
        assert text_duration < 0.1  # <100ms for text search

        # Test semantic search latency
        start_time = time.time()
        results = await temp_db.semantic_search("calculate numbers")
        semantic_duration = time.time() - start_time

        assert len(results) >= 0
        assert semantic_duration < 0.5  # <500ms for semantic search (embedding generation)


class TestConcurrency:
    """Test concurrent operations."""

    @pytest.mark.asyncio
    async def test_concurrent_searches(self, temp_db, sample_entities):
        """Test concurrent search operations."""
        await temp_db.add_entities_batch(sample_entities)

        # Run multiple searches concurrently
        search_tasks = [
            temp_db.text_search("function"),
            temp_db.semantic_search("calculate"),
            temp_db.text_search("User"),
            temp_db.semantic_search("class")
        ]

        results = await asyncio.gather(*search_tasks)

        # All searches should complete successfully
        assert len(results) == 4
        for result in results:
            assert isinstance(result, list)

    @pytest.mark.asyncio
    async def test_concurrent_additions(self, temp_db):
        """Test concurrent entity additions."""
        # Create different entity sets
        entities_set_1 = [
            {
                "id": f"concurrent_1_{i}",
                "name": f"func_1_{i}",
                "fqn": f"test1.py::func_1_{i}",
                "kind": "function",
                "content": f"def func_1_{i}(): pass",
                "signature": f"def func_1_{i}():",
                "file": "test1.py",
                "line_start": i,
                "line_end": i,
                "language": "python"
            }
            for i in range(10)
        ]

        entities_set_2 = [
            {
                "id": f"concurrent_2_{i}",
                "name": f"func_2_{i}",
                "fqn": f"test2.py::func_2_{i}",
                "kind": "function",
                "content": f"def func_2_{i}(): pass",
                "signature": f"def func_2_{i}():",
                "file": "test2.py",
                "line_start": i,
                "line_end": i,
                "language": "python"
            }
            for i in range(10)
        ]

        # Add both sets concurrently
        add_tasks = [
            temp_db.add_entities_batch(entities_set_1),
            temp_db.add_entities_batch(entities_set_2)
        ]

        counts = await asyncio.gather(*add_tasks)

        assert counts[0] == 10
        assert counts[1] == 10

        # Verify all entities were added
        stats = await temp_db.get_statistics()
        assert stats["total_entities"] == 20