// SOURCE: Nested structures with missing closing braces
function processData() {
    const config = {
        database: {
            host: "localhost",
            port: 5432,
            credentials: {
                username: "admin",
                password: "secret"
                // Missing closing brace for credentials
            // Missing closing brace for database
        // Missing closing brace for config
    return config;
}
