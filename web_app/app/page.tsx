import Image from 'next/image'
import { Inter } from '@next/font/google'
import styles from './page.module.css'

const inter = Inter({ subsets: ['latin'] })

export default function Home() {
  return (
    <div className='w-full bg-white'>
      <main className='flex justify-center items-center m-auto h-screen flex-col'>
        <div className='text-8xl text-blue-900'>
          Welcome to <a href='https://nextjs.org'>Next.js 13!</a>
        </div>
        <div className='text-4xl my-10 '>
          This is a Tailwind Example with Next.js 13
        </div>
      </main>

      <footer className={styles.footer}>
        <a
          href='https://vercel.com?utm_source=create-next-app&utm_medium=default-template&utm_campaign=create-next-app'
          target='_blank'
          rel='noopener noreferrer'
        >
          Powered by{" "}
          <span className={styles.logo}>
            <Image src='/vercel.svg' alt='Vercel Logo' width={72} height={16} />
          </span>
        </a>
      </footer>
    </div>
  )
}
