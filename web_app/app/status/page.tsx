import React from 'react'
import { Inter } from '@next/font/google'
import StatusComponent from './status-component'

const inter = Inter({ subsets: ['latin'] })
const url = 'https://our-places-app-api-rs-applxgnleq-uc.a.run.app/ping';

async function getData() {
    const res = await fetch(url);

    if (!res.ok) {
        // This will activate the closest `error.js` Error Boundary
        throw new Error('Failed to fetch data');
    }

    return res.json();
}

export default async function Page() {
    const data = await getData();
    return <main>{data.status}</main>;
}