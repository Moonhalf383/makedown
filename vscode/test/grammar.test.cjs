const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const oniguruma = require('vscode-oniguruma');
const { Registry, parseRawGrammar } = require('vscode-textmate');

const root = path.resolve(__dirname, '..');
const grammars = {
  'source.markfile': 'markfile.tmLanguage.json',
  'text.html.markdown.mkd-template': 'mkd-template.tmLanguage.json'
};

// 使用真实的 Oniguruma 和 TextMate 分词器验证高亮作用域。
async function registry() {
  const wasm = fs.readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm')).buffer;
  await oniguruma.loadWASM(wasm);
  return new Registry({
    onigLib: Promise.resolve({ createOnigScanner: patterns => new oniguruma.OnigScanner(patterns), createOnigString: text => new oniguruma.OnigString(text) }),
    loadGrammar: async scope => {
      if (!grammars[scope]) return null;
      const file = path.join(root, 'syntaxes', grammars[scope]);
      return parseRawGrammar(fs.readFileSync(file, 'utf8'), file);
    }
  });
}

function scopes(grammar, line) {
  return grammar.tokenizeLine(line).tokens.map(token => ({ text: line.slice(token.startIndex, token.endIndex), scopes: token.scopes.join(' ') }));
}

test('Markfile separates titles, dependencies, anchors, and specifications', async () => {
  const grammar = await (await registry()).loadGrammar('source.markfile');
  assert.ok(scopes(grammar, '---').some(token => token.scopes.includes('punctuation.separator.markfile')));
  assert.ok(scopes(grammar, '# build').some(token => token.text === '#' && token.scopes.includes('punctuation.definition.heading.markfile')));
  assert.ok(scopes(grammar, '> self::api::publish as output').some(token => token.scopes.includes('keyword.control.anchor.markfile')));
  assert.ok(scopes(grammar, '> self::api::publish as output').some(token => token.scopes.includes('keyword.control.alias.markfile')));
  assert.ok(scopes(grammar, '- accepted').some(token => token.scopes.includes('string.unquoted.specification.markfile')));
  assert.ok(!scopes(grammar, '  # description').some(token => token.scopes.includes('meta.target.markfile')));
});

test('Markdown templates highlight their own delimiters and fields', async () => {
  const grammar = await (await registry()).loadGrammar('text.html.markdown.mkd-template');
  assert.ok(scopes(grammar, '{{ plan.root_target }}').some(token => token.scopes.includes('variable.other.template.mkd')));
  assert.ok(scopes(grammar, '# Plan: {{ plan.root_target }}').some(token => token.scopes.includes('punctuation.definition.heading.markdown')));
  assert.ok(scopes(grammar, '# Plan: {{ plan.root_target }}').some(token => token.scopes.includes('variable.other.template.mkd')));
  assert.ok(scopes(grammar, 'Plan: {{ plan.root_target }}').some(token => token.scopes.includes('variable.other.template.mkd')));
  assert.ok(scopes(grammar, '| {{ target.id }} |').some(token => token.scopes.includes('variable.other.template.mkd')));
  assert.ok(scopes(grammar, '{% for stage in plan.stages %}').some(token => token.scopes.includes('keyword.control.template.mkd')));
  assert.ok(scopes(grammar, '{# comment #}').some(token => token.scopes.includes('comment.block.template.mkd')));
});
