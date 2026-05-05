export function Badge({ label }) {
    function handleClick() {
        return format(label);
    }

    return <button onClick={handleClick}>{format(label)}</button>;
}

function format(value) {
    return value.trim();
}
