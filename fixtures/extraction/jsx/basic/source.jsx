export function Badge({ label }) {
    function handleClick() {
        return format(label);
    }

    return <button onClick={handleClick}>{format(label)}</button>;
}

/**
 * Format the badge label.
 * @returns {string}
 */
function format(value) {
    return value.trim();
}
