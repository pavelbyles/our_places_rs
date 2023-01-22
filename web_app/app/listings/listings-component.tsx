export default function ListingsCompoment() {
    const listingsArr: readonly string[] = ['Palm Beach', 'Ei8ht 13', 'Ei8ht 23', '20 South - 407', '20 South - 409', 'Mona'];

    return (
        <ul>
            {listingsArr.map((item, index) => (
                <li key="index">{item}</li>
            ))}
        </ul>);
};