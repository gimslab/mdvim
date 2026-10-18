#!/usr/bin/env node
/**
 * mdvim ctl CLI bridge
 * Connects to running mdvim app via Unix Domain Socket and executes commands or queries state.
 */

import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import fs from 'node:fs';

function getSocketPath() {
  if (process.platform === 'win32') {
    return path.join(os.tmpdir(), 'mdvim.sock');
  }
  const uid = typeof process.getuid === 'function' ? process.getuid() : 1000;
  return `/tmp/mdvim-${uid}.sock`;
}

async function sendIpcRequest(method, params = null) {
  const sockPath = getSocketPath();

  if (!fs.existsSync(sockPath)) {
    throw new Error(
      `Cannot connect to mdvim: Socket not found at ${sockPath}.\nMake sure the mdvim desktop app is currently running.`
    );
  }

  return new Promise((resolve, reject) => {
    const client = net.createConnection({ path: sockPath }, () => {
      const payload = JSON.stringify({ method, params }) + '\n';
      client.write(payload);
    });

    let buffer = '';

    client.on('data', (data) => {
      buffer += data.toString('utf-8');
      if (buffer.includes('\n')) {
        client.end();
      }
    });

    client.on('end', () => {
      try {
        const trimmed = buffer.trim();
        if (!trimmed) {
          resolve({});
          return;
        }
        const json = JSON.parse(trimmed);
        resolve(json);
      } catch (err) {
        reject(new Error(`Failed to parse response: ${buffer}`));
      }
    });

    client.on('error', (err) => {
      reject(err);
    });
  });
}

function printHelp() {
  console.log(`
mdvim ctl - External AI & Automation CLI for mdvim

Usage:
  mdvim ctl get-state
      Print active tab, raw markdown, projected lines, cursor, and Vim mode as JSON.

  mdvim ctl list-commands
      List all registered commands, their titles, and default shortcuts.

  mdvim ctl exec <command-id> [args-json]
      Execute a command in mdvim (e.g. view.toggleMode, format.toggleBold, tab.new).

  mdvim ctl send-keys <keys>
      Send raw Vim key sequence to Neovim (e.g. 'yy', 'p', 'iHello<Esc>', ':w<CR>').

  mdvim ctl insert-text <text>
      Paste or stream text directly into active buffer at current cursor position.
`);
}

async function main() {
  const args = process.argv.slice(2);
  const cmd = args[0];

  if (!cmd || cmd === '--help' || cmd === '-h' || cmd === 'help') {
    printHelp();
    process.exit(0);
  }

  try {
    switch (cmd) {
      case 'get-state': {
        const res = await sendIpcRequest('get-state');
        console.log(JSON.stringify(res.state, null, 2));
        break;
      }
      case 'list-commands': {
        const res = await sendIpcRequest('list-commands');
        if (res.status === 'ok') {
          console.log(JSON.stringify(res.commands, null, 2));
        } else {
          console.error(res.message || 'Error listing commands');
          process.exit(1);
        }
        break;
      }
      case 'exec': {
        const commandId = args[1];
        if (!commandId) {
          console.error('Error: command-id required. Usage: mdvim ctl exec <command-id> [args-json]');
          process.exit(1);
        }
        let parsedArgs = null;
        if (args[2]) {
          try {
            parsedArgs = JSON.parse(args[2]);
          } catch {
            parsedArgs = args[2];
          }
        }
        const res = await sendIpcRequest('exec', { id: commandId, args: parsedArgs });
        if (res.status === 'ok') {
          console.log(JSON.stringify(res, null, 2));
        } else {
          console.error(`Error executing ${commandId}:`, res.message);
          process.exit(1);
        }
        break;
      }
      case 'send-keys': {
        const keys = args.slice(1).join(' ');
        if (!keys) {
          console.error('Error: keys required. Usage: mdvim ctl send-keys <keys>');
          process.exit(1);
        }
        const res = await sendIpcRequest('send-keys', { keys });
        console.log(JSON.stringify(res, null, 2));
        break;
      }
      case 'insert-text': {
        const raw = args.slice(1).join(' ');
        if (!raw) {
          console.error('Error: text required. Usage: mdvim ctl insert-text <text>');
          process.exit(1);
        }
        // Unescape literal \n, \r, \t if passed in single quotes from shell
        const text = raw
          .replace(/\\r\\n/g, '\n')
          .replace(/\\n/g, '\n')
          .replace(/\\t/g, '\t');
        const res = await sendIpcRequest('insert-text', { text });
        console.log(JSON.stringify(res, null, 2));
        break;
      }
      default:
        console.error(`Unknown subcommand: ${cmd}`);
        printHelp();
        process.exit(1);
    }
  } catch (err) {
    console.error('Error:', err.message);
    process.exit(1);
  }
}

main();
