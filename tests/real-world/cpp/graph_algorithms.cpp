/**
 * @file graph_algorithms.cpp
 * @brief Implementation of various graph algorithms using modern C++
 * @details This file contains implementations of common graph algorithms
 * including DFS, BFS, Dijkstra's algorithm, and topological sorting
 * using STL containers and object-oriented design patterns.
 */

#include <iostream>
#include <vector>
#include <queue>
#include <stack>
#include <unordered_map>
#include <unordered_set>
#include <algorithm>
#include <limits>
#include <functional>

/**
 * @brief Graph class implementing adjacency list representation
 * @tparam T Type of vertex data (must be hashable for unordered_map)
 */
template <typename T>
class Graph {
private:
    std::unordered_map<T, std::vector<std::pair<T, int>>> adj_list;
    bool is_directed;

public:
    /**
     * @brief Constructor for Graph
     * @param directed Whether the graph is directed (default: false)
     */
    explicit Graph(bool directed = false) : is_directed(directed) {}

    /**
     * @brief Add an edge to the graph
     * @param from Source vertex
     * @param to Destination vertex
     * @param weight Edge weight (default: 1)
     */
    void add_edge(const T& from, const T& to, int weight = 1) {
        adj_list[from].emplace_back(to, weight);

        // Add vertices if they don't exist
        if (adj_list.find(to) == adj_list.end()) {
            adj_list[to] = std::vector<std::pair<T, int>>();
        }

        // For undirected graphs, add reverse edge
        if (!is_directed) {
            adj_list[to].emplace_back(from, weight);
        }
    }

    /**
     * @brief Get all vertices in the graph
     * @return Vector containing all vertices
     */
    std::vector<T> get_vertices() const {
        std::vector<T> vertices;
        vertices.reserve(adj_list.size());

        for (const auto& pair : adj_list) {
            vertices.push_back(pair.first);
        }

        return vertices;
    }

    /**
     * @brief Get neighbors of a vertex
     * @param vertex The vertex to get neighbors for
     * @return Vector of pairs (neighbor, weight)
     */
    const std::vector<std::pair<T, int>>& get_neighbors(const T& vertex) const {
        static const std::vector<std::pair<T, int>> empty_vector;
        auto it = adj_list.find(vertex);
        return (it != adj_list.end()) ? it->second : empty_vector;
    }

    /**
     * @brief Check if graph contains a vertex
     * @param vertex Vertex to check
     * @return True if vertex exists, false otherwise
     */
    bool contains_vertex(const T& vertex) const {
        return adj_list.find(vertex) != adj_list.end();
    }

    /**
     * @brief Get the number of vertices
     * @return Number of vertices in the graph
     */
    size_t vertex_count() const {
        return adj_list.size();
    }

    /**
     * @brief Depth-First Search traversal
     * @param start_vertex Starting vertex for DFS
     * @return Vector containing vertices in DFS order
     */
    std::vector<T> dfs(const T& start_vertex) const {
        if (!contains_vertex(start_vertex)) {
            return {};
        }

        std::vector<T> result;
        std::unordered_set<T> visited;
        std::stack<T> stack;

        stack.push(start_vertex);

        while (!stack.empty()) {
            T current = stack.top();
            stack.pop();

            if (visited.find(current) == visited.end()) {
                visited.insert(current);
                result.push_back(current);

                // Add neighbors to stack (in reverse order for consistent ordering)
                const auto& neighbors = get_neighbors(current);
                for (auto it = neighbors.rbegin(); it != neighbors.rend(); ++it) {
                    if (visited.find(it->first) == visited.end()) {
                        stack.push(it->first);
                    }
                }
            }
        }

        return result;
    }

    /**
     * @brief Breadth-First Search traversal
     * @param start_vertex Starting vertex for BFS
     * @return Vector containing vertices in BFS order
     */
    std::vector<T> bfs(const T& start_vertex) const {
        if (!contains_vertex(start_vertex)) {
            return {};
        }

        std::vector<T> result;
        std::unordered_set<T> visited;
        std::queue<T> queue;

        queue.push(start_vertex);
        visited.insert(start_vertex);

        while (!queue.empty()) {
            T current = queue.front();
            queue.pop();
            result.push_back(current);

            for (const auto& neighbor_pair : get_neighbors(current)) {
                const T& neighbor = neighbor_pair.first;
                if (visited.find(neighbor) == visited.end()) {
                    visited.insert(neighbor);
                    queue.push(neighbor);
                }
            }
        }

        return result;
    }

    /**
     * @brief Dijkstra's shortest path algorithm
     * @param start_vertex Starting vertex
     * @return Pair of (distances map, predecessors map)
     */
    std::pair<std::unordered_map<T, int>, std::unordered_map<T, T>>
    dijkstra(const T& start_vertex) const {
        std::unordered_map<T, int> distances;
        std::unordered_map<T, T> predecessors;

        // Priority queue: (distance, vertex)
        std::priority_queue<std::pair<int, T>,
                          std::vector<std::pair<int, T>>,
                          std::greater<std::pair<int, T>>> pq;

        // Initialize distances
        for (const auto& vertex_pair : adj_list) {
            distances[vertex_pair.first] = std::numeric_limits<int>::max();
        }

        distances[start_vertex] = 0;
        pq.emplace(0, start_vertex);

        while (!pq.empty()) {
            auto [current_dist, current_vertex] = pq.top();
            pq.pop();

            // Skip if we've already found a better path
            if (current_dist > distances[current_vertex]) {
                continue;
            }

            // Check all neighbors
            for (const auto& [neighbor, weight] : get_neighbors(current_vertex)) {
                int new_distance = distances[current_vertex] + weight;

                if (new_distance < distances[neighbor]) {
                    distances[neighbor] = new_distance;
                    predecessors[neighbor] = current_vertex;
                    pq.emplace(new_distance, neighbor);
                }
            }
        }

        return {distances, predecessors};
    }

    /**
     * @brief Get shortest path between two vertices using Dijkstra's result
     * @param predecessors Predecessors map from dijkstra()
     * @param start_vertex Starting vertex
     * @param end_vertex Ending vertex
     * @return Vector containing the shortest path
     */
    std::vector<T> get_shortest_path(
        const std::unordered_map<T, T>& predecessors,
        const T& start_vertex,
        const T& end_vertex) const {

        std::vector<T> path;
        T current = end_vertex;

        // Build path backwards
        while (current != start_vertex && predecessors.find(current) != predecessors.end()) {
            path.push_back(current);
            current = predecessors.at(current);
        }

        if (current == start_vertex) {
            path.push_back(start_vertex);
            std::reverse(path.begin(), path.end());
        } else {
            // No path exists
            return {};
        }

        return path;
    }

    /**
     * @brief Topological sort using DFS (only for directed acyclic graphs)
     * @return Vector containing vertices in topological order
     */
    std::vector<T> topological_sort() const {
        if (!is_directed) {
            throw std::runtime_error("Topological sort only works on directed graphs");
        }

        std::unordered_set<T> visited;
        std::stack<T> result_stack;

        // Visit all vertices
        for (const auto& vertex_pair : adj_list) {
            if (visited.find(vertex_pair.first) == visited.end()) {
                topological_sort_util(vertex_pair.first, visited, result_stack);
            }
        }

        // Convert stack to vector
        std::vector<T> result;
        while (!result_stack.empty()) {
            result.push_back(result_stack.top());
            result_stack.pop();
        }

        return result;
    }

private:
    /**
     * @brief Utility function for topological sort
     * @param vertex Current vertex
     * @param visited Set of visited vertices
     * @param result_stack Stack to store the result
     */
    void topological_sort_util(const T& vertex,
                              std::unordered_set<T>& visited,
                              std::stack<T>& result_stack) const {
        visited.insert(vertex);

        // Visit all neighbors
        for (const auto& neighbor_pair : get_neighbors(vertex)) {
            if (visited.find(neighbor_pair.first) == visited.end()) {
                topological_sort_util(neighbor_pair.first, visited, result_stack);
            }
        }

        // Add current vertex to result after visiting all neighbors
        result_stack.push(vertex);
    }
};

/**
 * @brief Utility function to print a vector
 * @tparam T Type of elements in the vector
 * @param vec Vector to print
 * @param label Label to print before the vector
 */
template <typename T>
void print_vector(const std::vector<T>& vec, const std::string& label = "") {
    if (!label.empty()) {
        std::cout << label << ": ";
    }

    for (size_t i = 0; i < vec.size(); ++i) {
        std::cout << vec[i];
        if (i < vec.size() - 1) std::cout << " -> ";
    }
    std::cout << std::endl;
}

/**
 * @brief Main function demonstrating graph algorithms
 */
int main() {
    std::cout << "=== Graph Algorithms Demo ===" << std::endl;

    // Create undirected graph for BFS/DFS testing
    Graph<std::string> undirected_graph(false);

    // Add edges to create a sample graph
    undirected_graph.add_edge("A", "B", 4);
    undirected_graph.add_edge("A", "C", 2);
    undirected_graph.add_edge("B", "C", 1);
    undirected_graph.add_edge("B", "D", 5);
    undirected_graph.add_edge("C", "D", 8);
    undirected_graph.add_edge("C", "E", 10);
    undirected_graph.add_edge("D", "E", 2);

    std::cout << "\n--- Undirected Graph Traversals ---" << std::endl;

    // Test DFS
    auto dfs_result = undirected_graph.dfs("A");
    print_vector(dfs_result, "DFS from A");

    // Test BFS
    auto bfs_result = undirected_graph.bfs("A");
    print_vector(bfs_result, "BFS from A");

    std::cout << "\n--- Dijkstra's Algorithm ---" << std::endl;

    // Test Dijkstra's algorithm
    auto [distances, predecessors] = undirected_graph.dijkstra("A");

    std::cout << "Shortest distances from A:" << std::endl;
    for (const auto& [vertex, distance] : distances) {
        std::cout << "  A -> " << vertex << ": " << distance << std::endl;
    }

    // Get shortest path from A to E
    auto shortest_path = undirected_graph.get_shortest_path(predecessors, "A", "E");
    print_vector(shortest_path, "Shortest path A to E");

    std::cout << "\n--- Directed Graph - Topological Sort ---" << std::endl;

    // Create directed graph for topological sort
    Graph<int> directed_graph(true);

    directed_graph.add_edge(5, 2);
    directed_graph.add_edge(5, 0);
    directed_graph.add_edge(4, 0);
    directed_graph.add_edge(4, 1);
    directed_graph.add_edge(2, 3);
    directed_graph.add_edge(3, 1);

    try {
        auto topo_order = directed_graph.topological_sort();
        print_vector(topo_order, "Topological Order");
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }

    std::cout << "\n--- Graph Statistics ---" << std::endl;
    std::cout << "Undirected graph vertices: " << undirected_graph.vertex_count() << std::endl;
    std::cout << "Directed graph vertices: " << directed_graph.vertex_count() << std::endl;

    return 0;
}