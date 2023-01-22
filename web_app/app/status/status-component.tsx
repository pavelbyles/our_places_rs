import { loadStatus } from '../lib/api';

async function StatusComponent() {
    const status = await loadStatus();

    return status ? (
        <div>{JSON.stringify(status)}</div>
    ) :
        (
            <div>No status result</div>
        )

};

export default StatusComponent;