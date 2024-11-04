import "./App.css";
import { commands } from './bindings'


function App() {

  async function greet() {
    await commands.updateConfig(JSON.stringify({ configured: true }))
  }

  return (
    <main className="container">
      <h1>Welcome to Tauri + React</h1>

      <div className="row">
        <a >
          <img src="/vite.svg" className="logo vite" alt="Vite logo" />
        </a>
        <a >
          <img src="/tauri.svg" className="logo tauri" alt="Tauri logo" />
        </a>
        <a >
        </a>
        <button onClick={greet}>CLick me</button>
      </div>
      <p>Click on the Tauri, Vite, and React logos to learn more.</p>
    </main>
  );
}

export default App;
