const url: string = 'https://our-places-app-api-rs-applxgnleq-uc.a.run.app/ping';

interface StatusResult {
    result: string;
}

export async function loadStatus(): Promise<StatusResult> {
    const response = await fetch(url);

    if (!response.ok) {
        throw new Error('Failed to fetch data');
    }

    const result = await response.json() as StatusResult;

    return result;
}