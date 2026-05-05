type Props = {
    label: string;
};

export function Badge(props: Props) {
    return <span>{format(props.label)}</span>;
}

function format(value: string): string {
    return value.trim();
}
