// SOURCE: Multi-property object with missing closing brace
function getData() {
    const obj = {
        name: "Test",
        email: "test@example.com",
        status: "active"
    }
        // Missing closing brace causes parse error
    return obj;
}
