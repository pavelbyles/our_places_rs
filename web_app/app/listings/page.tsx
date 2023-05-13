import React from 'react'
//import Head from 'next/head'
import { Inter } from 'next/font/google'
import ListingsComponent from './listings-component';

const inter = Inter({ subsets: ['latin'] })

export default function page() {
  return (
    <ListingsComponent />
  )
}