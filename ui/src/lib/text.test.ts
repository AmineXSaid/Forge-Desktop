import { describe, expect, it } from 'vitest';
import { diffLines, counts } from './diff';
import { markdown } from './markdown';

describe('diffLines', () => {
  it('reconstructs both sides for random inputs', () => {
    const rand = (n: number) => Array.from({ length: n }, () => 'abcde'[Math.floor(Math.random() * 5)]);
    for (let t = 0; t < 300; t++) {
      const a = rand(Math.floor(Math.random() * 30));
      const b = rand(Math.floor(Math.random() * 30));
      const ops = diffLines(a.join('\n'), b.join('\n'));
      expect(ops.filter((o) => o.t !== '+').map((o) => o.text)).toEqual(a);
      expect(ops.filter((o) => o.t !== '-').map((o) => o.text)).toEqual(b);
    }
  });
  it('counts a changed line as one out and one in', () => {
    expect(counts(diffLines('one\ntwo\n', 'one\nTWO\nthree\n'))).toEqual({ add: 2, del: 1 });
  });
});

describe('markdown', () => {
  it('never lets model output become HTML', () => {
    const html = markdown('# Hi <script>alert(1)</script>\n\n`<b>` and <img src=x onerror=alert(1)>');
    expect(html).not.toMatch(/<script|<img|<b>/);
    expect(html).toContain('&lt;script&gt;');
  });
  it('links carry their target as data, not as href', () => {
    expect(markdown('[x](javascript:alert(1))')).not.toContain('<a');
    expect(markdown('[x](https://a.b)')).toContain('<a data-href="https://a.b"');
    expect(markdown('see https://a.b/c')).toContain('data-href="https://a.b/c"');
  });
  it('renders code, lists and tables', () => {
    const html = markdown('```js\nlet a = 1;\n```\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |');
    expect(html).toContain('<pre data-lang="js"><code>let a = 1;</code></pre>');
    expect(html).toContain('<ul><li>one</li><li>two</li></ul>');
    expect(html).toContain('<td>1</td>');
  });
});
