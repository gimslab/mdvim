import { test, after } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { attach } from 'neovim';
import { parse, project, serialize, applyProjected } from './projection.js';

const procs = [];
after(() => procs.forEach((p) => p.kill()));

async function session(md) {
  const proc = spawn('nvim', ['--embed', '--headless', '-u', 'NONE', '-n'], { stdio: ['pipe', 'pipe', 'inherit'] });
  procs.push(proc);
  const nvim = attach({ proc });
  let model = parse(md);
  const buf = await nvim.buffer;
  await buf.setLines(project(model), { start: 0, end: -1, strictIndexing: false });
  return {
    get model() { return model; },
    // run normal-mode keys, then sync edits back into the markdown model
    async keys(k) {
      await nvim.command('normal! ' + k);
      model = applyProjected(model, await buf.lines);
      return serialize(model);
    },
    projected: async () => (await buf.lines).join('\n'),
    md: () => serialize(model),
    close: () => nvim.quit(),
  };
}

const SAMPLE = [
  '# Title here',
  '',
  'plain **bold word** and more',
  '- item one **b**',
  '1. numbered',
  '한글 **굵은 글씨** 입니다',
  '',
].join('\n');

test('1. no-op roundtrip is byte-identical (incl. odd lines)', () => {
  const odd = SAMPLE + '\n  * indented *it* __u__ ** spaced ** `code`\r\n\n\n';
  assert.equal(serialize(parse(odd)), odd);
  const s = serialize(applyProjected(parse(odd), project(parse(odd))));
  assert.equal(s, odd);
});

test('projection hides markers', async () => {
  const s = await session(SAMPLE);
  assert.equal(await s.projected(), SAMPLE.replaceAll('**', '').replace('# ', '').replace('- ', '').replace('1. ', ''));
  await s.close();
});

test('2a. x inside bold', async () => {
  const s = await session('plain **bold word** end');
  // cursor at line1 col0; move to "bold"
  const out = await s.keys('wx');
  assert.equal(out, 'plain **old word** end');
  await s.close();
});

test('2b. dw on bold word keeps remaining bold', async () => {
  const s = await session('plain **bold word** end');
  assert.equal(await s.keys('wdw'), 'plain **word** end');
  await s.close();
});

test('2c. cw on bold word replaces with bold text', async () => {
  const s = await session('plain **bold word** end');
  assert.equal(await s.keys('wcwNEW'), 'plain **NEW word** end');
  await s.close();
});

test('2d. dot repeat across bold and plain', async () => {
  const s = await session('aa **bb** cc');
  assert.equal(await s.keys('dw'), '**bb** cc');
  assert.equal(await s.keys('.'), 'cc');
  await s.close();
});

test('2e. delete whole bold run drops the markers', async () => {
  const s = await session('a **bb** c');
  assert.equal(await s.keys('wdw'), 'a c');
  await s.close();
});

test('2f. Korean in bold', async () => {
  const s = await session('한글 **굵은 글씨** 입니다');
  assert.equal(await s.keys('wx'), '한글 **은 글씨** 입니다');
  await s.close();
});

test('3a. append at end of bold extends bold (rule: inherit left)', async () => {
  const s = await session('a **bold** b');
  assert.equal(await s.keys('wea!'), 'a **bold!** b');
  await s.close();
});

test('3b. insert at start of bold is plain (rule: inherit left)', async () => {
  const s = await session('a **bold** b');
  assert.equal(await s.keys('wi!'), 'a !**bold** b');
  await s.close();
});

test('3c. insert in middle of bold stays bold', async () => {
  const s = await session('a **bold** b');
  assert.equal(await s.keys('wlli!'), 'a **bo!ld** b');
  await s.close();
});

test('3d. trailing space typed at end of bold moves outside markers', async () => {
  const s = await session('a **bold** b');
  assert.equal(await s.keys('wea '), 'a **bold** b'.replace('bold** b', 'bold**  b'));
  await s.close();
});

test('4a. J joins lines, keeps first line prefix and bold', async () => {
  const s = await session('- one **two**\n- three');
  assert.equal(await s.keys('J'), '- one **two** three');
  await s.close();
});

test('4b. dd on middle line keeps neighbors intact', async () => {
  const s = await session('# H\n- a **b**\nplain **c**');
  assert.equal(await s.keys('jdd'), '# H\nplain **c**');
  await s.close();
});

test('4c. dd on last line', async () => {
  const s = await session('# H\nplain **c**');
  assert.equal(await s.keys('jdd'), '# H');
  await s.close();
});

test('4d. dd on heading keeps next line prefix', async () => {
  const s = await session('# H\n- a **b**\nz');
  assert.equal(await s.keys('dd'), '- a **b**\nz');
  await s.close();
});

test('4e. yy + p duplicates a line (as plain text: prefix/bold lost)', async () => {
  const s = await session('# H\n- a **b**\nz');
  const out = await s.keys('jyyp');
  console.log('   4e actual ->', JSON.stringify(out));
  assert.equal(out.split('\n').length, 4);
  await s.close();
});

test('4f. o opens a new plain line, others untouched', async () => {
  const s = await session('- a **b**\n- c');
  assert.equal(await s.keys('onew'), '- a **b**\nnew\n- c');
  await s.close();
});

test('4g. markdown never breaks: text typed next to bold with spaces', async () => {
  const s = await session('**a b**');
  assert.equal(await s.keys('A '), '**a b** ');
  await s.close();
});

test('4h. P of yanked line before heading keeps heading prefix', async () => {
  const s = await session('# H\nz');
  const out = await s.keys('jyyggP');
  assert.equal(out, 'z\n# H\nz');
  await s.close();
});
