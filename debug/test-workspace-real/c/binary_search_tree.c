/**
 * @file binary_search_tree.c
 * @author The Algorithms - C
 * @brief Implementation of Binary Search Tree data structure
 * @details
 * Binary Search Tree (BST) is a node-based binary tree data structure with the
 * following properties:
 * - Left subtree contains nodes with keys less than parent node
 * - Right subtree contains nodes with keys greater than parent node
 * - Both left and right subtrees must also be binary search trees
 */

#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>

/**
 * @brief Structure representing a node in the binary search tree
 */
typedef struct node {
    int data;               /**< Data stored in the node */
    struct node *left;      /**< Pointer to left child */
    struct node *right;     /**< Pointer to right child */
} Node;

/**
 * @brief Structure representing the binary search tree
 */
typedef struct {
    Node *root;             /**< Root node of the tree */
    int size;               /**< Number of nodes in the tree */
} BST;

/**
 * @brief Create a new node with given data
 * @param data The data to store in the node
 * @return Pointer to the newly created node
 */
Node* create_node(int data) {
    Node *new_node = (Node*)malloc(sizeof(Node));
    if (new_node == NULL) {
        fprintf(stderr, "Memory allocation failed\n");
        return NULL;
    }

    new_node->data = data;
    new_node->left = NULL;
    new_node->right = NULL;

    return new_node;
}

/**
 * @brief Initialize a binary search tree
 * @return Pointer to initialized BST structure
 */
BST* init_bst() {
    BST *tree = (BST*)malloc(sizeof(BST));
    if (tree == NULL) {
        fprintf(stderr, "Memory allocation failed for BST\n");
        return NULL;
    }

    tree->root = NULL;
    tree->size = 0;

    return tree;
}

/**
 * @brief Insert a node into the BST recursively
 * @param root Current node being examined
 * @param data Data to insert
 * @return Pointer to the node (modified tree)
 */
Node* insert_recursive(Node *root, int data) {
    // Base case: empty tree or reached leaf
    if (root == NULL) {
        return create_node(data);
    }

    // Recursive case: traverse based on BST property
    if (data < root->data) {
        root->left = insert_recursive(root->left, data);
    }
    else if (data > root->data) {
        root->right = insert_recursive(root->right, data);
    }

    // Return the unchanged root pointer
    return root;
}

/**
 * @brief Insert data into the BST
 * @param tree Pointer to the BST
 * @param data Data to insert
 * @return true if insertion successful, false otherwise
 */
bool insert(BST *tree, int data) {
    if (tree == NULL) {
        return false;
    }

    int initial_size = tree->size;
    tree->root = insert_recursive(tree->root, data);

    // Check if insertion actually happened (no duplicates)
    if (tree->root != NULL) {
        tree->size++;
        return true;
    }

    return false;
}

/**
 * @brief Find the minimum value node in a subtree
 * @param node Root of the subtree
 * @return Pointer to the node with minimum value
 */
Node* find_min(Node *node) {
    if (node == NULL) {
        return NULL;
    }

    while (node->left != NULL) {
        node = node->left;
    }

    return node;
}

/**
 * @brief Delete a node from BST recursively
 * @param root Current node being examined
 * @param data Data to delete
 * @return Pointer to the modified tree
 */
Node* delete_recursive(Node *root, int data) {
    // Base case: empty tree
    if (root == NULL) {
        return root;
    }

    // Recursive case: find the node to delete
    if (data < root->data) {
        root->left = delete_recursive(root->left, data);
    }
    else if (data > root->data) {
        root->right = delete_recursive(root->right, data);
    }
    else {
        // Node found - handle three cases

        // Case 1: Node with no children (leaf)
        if (root->left == NULL && root->right == NULL) {
            free(root);
            return NULL;
        }

        // Case 2: Node with one child
        else if (root->left == NULL) {
            Node *temp = root->right;
            free(root);
            return temp;
        }
        else if (root->right == NULL) {
            Node *temp = root->left;
            free(root);
            return temp;
        }

        // Case 3: Node with two children
        else {
            Node *temp = find_min(root->right);  // In-order successor

            // Copy the in-order successor's data to this node
            root->data = temp->data;

            // Delete the in-order successor
            root->right = delete_recursive(root->right, temp->data);
        }
    }

    return root;
}

/**
 * @brief Delete data from the BST
 * @param tree Pointer to the BST
 * @param data Data to delete
 * @return true if deletion successful, false otherwise
 */
bool delete(BST *tree, int data) {
    if (tree == NULL || tree->root == NULL) {
        return false;
    }

    Node *original_root = tree->root;
    tree->root = delete_recursive(tree->root, data);

    if (tree->root != original_root || search_recursive(tree->root, data) == NULL) {
        tree->size--;
        return true;
    }

    return false;
}

/**
 * @brief Search for a value in BST recursively
 * @param root Current node being examined
 * @param data Data to search for
 * @return Pointer to the found node, NULL if not found
 */
Node* search_recursive(Node *root, int data) {
    // Base cases: root is null or data is present at root
    if (root == NULL || root->data == data) {
        return root;
    }

    // Data is greater than root's data
    if (data > root->data) {
        return search_recursive(root->right, data);
    }

    // Data is smaller than root's data
    return search_recursive(root->left, data);
}

/**
 * @brief Search for data in the BST
 * @param tree Pointer to the BST
 * @param data Data to search for
 * @return true if found, false otherwise
 */
bool search(BST *tree, int data) {
    if (tree == NULL) {
        return false;
    }

    return search_recursive(tree->root, data) != NULL;
}

/**
 * @brief In-order traversal of BST (Left, Root, Right)
 * @param root Current node being visited
 */
void inorder_traversal(Node *root) {
    if (root != NULL) {
        inorder_traversal(root->left);   // Visit left subtree
        printf("%d ", root->data);        // Visit root
        inorder_traversal(root->right);   // Visit right subtree
    }
}

/**
 * @brief Pre-order traversal of BST (Root, Left, Right)
 * @param root Current node being visited
 */
void preorder_traversal(Node *root) {
    if (root != NULL) {
        printf("%d ", root->data);        // Visit root
        preorder_traversal(root->left);   // Visit left subtree
        preorder_traversal(root->right);  // Visit right subtree
    }
}

/**
 * @brief Calculate height of the BST
 * @param root Root node of the tree/subtree
 * @return Height of the tree (-1 for empty tree)
 */
int height(Node *root) {
    if (root == NULL) {
        return -1;
    }

    int left_height = height(root->left);
    int right_height = height(root->right);

    return 1 + ((left_height > right_height) ? left_height : right_height);
}

/**
 * @brief Free all nodes in the BST recursively
 * @param root Root node of the tree/subtree to free
 */
void free_tree(Node *root) {
    if (root != NULL) {
        free_tree(root->left);   // Free left subtree
        free_tree(root->right);  // Free right subtree
        free(root);              // Free current node
    }
}

/**
 * @brief Free the entire BST structure
 * @param tree Pointer to the BST to free
 */
void destroy_bst(BST *tree) {
    if (tree != NULL) {
        free_tree(tree->root);
        free(tree);
    }
}

/**
 * @brief Count total number of nodes in BST
 * @param root Root node of the tree/subtree
 * @return Number of nodes
 */
int count_nodes(Node *root) {
    if (root == NULL) {
        return 0;
    }

    return 1 + count_nodes(root->left) + count_nodes(root->right);
}

/**
 * @brief Find the maximum value in the BST
 * @param tree Pointer to the BST
 * @return Maximum value, or INT_MIN if tree is empty
 */
int find_max(BST *tree) {
    if (tree == NULL || tree->root == NULL) {
        return __INT_MIN__;
    }

    Node *current = tree->root;
    while (current->right != NULL) {
        current = current->right;
    }

    return current->data;
}

/**
 * @brief Find the minimum value in the BST
 * @param tree Pointer to the BST
 * @return Minimum value, or INT_MAX if tree is empty
 */
int find_minimum(BST *tree) {
    if (tree == NULL || tree->root == NULL) {
        return __INT_MAX__;
    }

    Node *min_node = find_min(tree->root);
    return min_node->data;
}

/**
 * @brief Main function to test the Binary Search Tree implementation
 */
int main() {
    BST *tree = init_bst();

    if (tree == NULL) {
        fprintf(stderr, "Failed to initialize BST\n");
        return 1;
    }

    // Test insertions
    printf("Testing BST operations...\n");

    int values[] = {50, 30, 20, 40, 70, 60, 80};
    int num_values = sizeof(values) / sizeof(values[0]);

    printf("Inserting values: ");
    for (int i = 0; i < num_values; i++) {
        printf("%d ", values[i]);
        insert(tree, values[i]);
    }
    printf("\n");

    printf("BST size: %d\n", tree->size);
    printf("BST height: %d\n", height(tree->root));

    // Test traversals
    printf("In-order traversal: ");
    inorder_traversal(tree->root);
    printf("\n");

    printf("Pre-order traversal: ");
    preorder_traversal(tree->root);
    printf("\n");

    // Test search
    int search_values[] = {25, 40, 80, 90};
    int num_search = sizeof(search_values) / sizeof(search_values[0]);

    printf("Searching for values:\n");
    for (int i = 0; i < num_search; i++) {
        bool found = search(tree, search_values[i]);
        printf("  %d: %s\n", search_values[i], found ? "Found" : "Not found");
    }

    // Test min/max
    printf("Minimum value: %d\n", find_minimum(tree));
    printf("Maximum value: %d\n", find_max(tree));

    // Test deletion
    printf("Deleting 20 and 30\n");
    delete(tree, 20);
    delete(tree, 30);

    printf("After deletion - In-order traversal: ");
    inorder_traversal(tree->root);
    printf("\n");

    printf("BST size after deletion: %d\n", tree->size);

    // Clean up
    destroy_bst(tree);
    printf("BST destroyed successfully\n");

    return 0;
}