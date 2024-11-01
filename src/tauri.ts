// import { writeTextFile, BaseDirectory } from '@tauri-apps/plugin-fs'
// import { Proxy, ProxyConfig, fetch } from '@tauri-apps/plugin-http'
import { commands } from './bindings'

export async function getSettings(): Promise<Record<string, any>> {
    const settings = await commands.getConfigContent()
    return JSON.parse(settings)
}