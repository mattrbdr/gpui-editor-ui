// Génère tests/fixtures/tiptap_markdown.json : pour chaque cas synthétique, le JSON ProseMirror produit
// par @tiptap/markdown et le Markdown qu'il réécrit à partir de ce JSON.
// Usage : npm install && npm run generate
import { writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { MarkdownManager } from '@tiptap/markdown'
import StarterKit from '@tiptap/starter-kit'
import Image from '@tiptap/extension-image'
import TaskItem from '@tiptap/extension-task-item'
import TaskList from '@tiptap/extension-task-list'

const manager = new MarkdownManager({
  extensions: [StarterKit, Image, TaskList, TaskItem.configure({ nested: true })],
})

// Texte brut de référence : feuilles de texte, saut de ligne après chaque bloc, lignes vides réduites.
function plainText(root) {
  const parts = []
  const blocks = ['paragraph', 'heading', 'blockquote', 'codeBlock', 'listItem', 'taskItem']
  const walk = (node) => {
    if (node.text) parts.push(node.text)
    for (const child of node.content || []) walk(child)
    if (blocks.includes(node.type)) parts.push('\n')
  }
  walk(root)
  return parts.join('').replace(/\n{3,}/g, '\n\n').trim()
}

const cases = {
  empty: '',
  paragraph: 'Un paragraphe simple.',
  two_paragraphs: 'Premier.\n\nSecond.',
  headings: '# Un\n\n## Deux\n\n### Trois\n\n###### Six',
  marks: 'du **gras**, de l’*italique*, du ~~barré~~ et du `code`.',
  nested_marks: '**gras et *italique* imbriqués** puis `code`',
  link: 'Voir [la page](https://example.com/a?b=c) et <https://example.org>.',
  link_with_bold: '[**lien gras**](https://example.com)',
  hard_break: 'ligne un  \nligne deux\\\nligne trois',
  bullet_list: '- un\n- deux\n  - sous-élément\n- trois',
  ordered_list: '1. un\n2. deux\n3. trois',
  ordered_list_start: '3. trois\n4. quatre',
  task_list: '- [ ] à faire\n- [x] fait\n  - [ ] imbriqué',
  code_block: '```js\nconst a = 1;\nconsole.log(a)\n```',
  code_block_plain: '```\nsans langage\n  indenté\n```',
  blockquote: '> citation\n>\n> sur deux paragraphes',
  horizontal_rule: 'avant\n\n---\n\naprès',
  image: '![texte alternatif](https://example.com/i.png "titre")',
  unicode: 'Accents éàùç, emoji 🙂 et 日本語.',
  escapes: 'Des \\* astérisques \\_ échappés et 1\\. un point.',
  soft_break: 'ligne un\nligne deux',
  underscore_emphasis: '_a_ et __b__',
  link_title: '[x](https://e.com "T")',
  image_no_alt: '![](https://e.com/i.png)',
  empty_code: '```\n```',
  indented_code: '    code\n    suite',
  tilde_fence: '~~~js\nx\n~~~',
  setext: 'Titre\n=====\n\nSous\n----',
  loose_list: '- a\n\n- b',
  ordered_ones: '1. a\n1. b',
  nested_quote: '> a\n> > b',
  quote_list: '> - a\n> - b',
  list_with_code: '- a\n\n  ```\n  x\n  ```',
  html_inline: 'a <b>x</b> c',
  html_block: '<div>x</div>',
  table: '|a|b|\n|-|-|\n|1|2|',
  entities: 'a &amp; b &lt; c &copy;',
  heading_marks: '## a **b**',
  adjacent_marks: '**a***b*',
  code_span_backticks: '`` a`b ``',
  intraword: 'a*b*c',
  hard_break_end: 'fin  \n',
  list_two_paragraphs: '- a\n\n  b',
  ordered_alpha_list: '1) a\n2) b',
  mixed_list_nesting: '- a\n  1. b\n  2. c\n- d',
  bold_link: '**[x](https://e.com)**',
  strike_bold: '~~**a**~~',
  mixed: '# Titre\n\nUn paragraphe avec **gras**.\n\n- a\n- b\n\n```\ncode\n```\n\n---\n',
}

// Cas limites construits directement en JSON (le Markdown n'est pas la source).
const t = (text, ...marks) => ({ type: 'text', text, ...(marks.length ? { marks: marks.map((m) => (typeof m === 'string' ? { type: m } : m)) } : {}) })
const para = (...content) => ({ type: 'paragraph', content })
const doc = (...content) => ({ type: 'doc', content })
const item = (...content) => ({ type: 'listItem', content })
const link = (attrs) => ({ type: 'link', attrs })
const docCases = {}
for (const c of '\\`*_{}[]()#+-.!|<>~"\'&$%^=:;,/?@') docCases[`char_${c.codePointAt(0)}`] = doc(para(t(`a${c}b`)))
for (const [i, s] of ['# x', '- x', '+ x', '1. x', '> x', '--- x', '```', '    x', '* x', '1) x', '***', '___', '|a|b|', '=== x'].entries()) docCases[`line_start_${i}`] = doc(para(t(s)))
Object.assign(docCases, {
  underline: doc(para(t('abc', 'underline'))),
  bold_italic: doc(para(t('abc', 'bold', 'italic'))),
  italic_bold: doc(para(t('abc', 'italic', 'bold'))),
  code_bold: doc(para(t('abc', 'code', 'bold'))),
  strike: doc(para(t('abc', 'strike'))),
  bold_spaces: doc(para(t(' abc ', 'bold'))),
  bold_adjacent: doc(para(t('a', 'bold'), t('b', 'bold'))),
  bold_then_italic: doc(para(t('a', 'bold'), t('b', 'italic'))),
  partial_overlap: doc(para(t('a', 'bold'), t('b', 'bold', 'italic'), t('c', 'italic'))),
  code_backtick: doc(para(t('a`b', 'code'))),
  code_two_backticks: doc(para(t('a``b', 'code'))),
  code_edge_backtick: doc(para(t('`a', 'code'))),
  link_title: doc(para(t('x', link({ href: 'https://e.com', title: 'T' })))),
  link_target: doc(para(t('x', link({ href: 'https://e.com', target: '_blank', rel: 'noopener', class: null })))),
  link_parens: doc(para(t('x', link({ href: 'https://e.com/a(b)c' })))),
  link_with_marks: doc(para(t('a ', link({ href: 'https://e.com' })), t('b', link({ href: 'https://e.com' }), 'bold'))),
  empty_paragraphs: doc({ type: 'paragraph' }, para(t('x')), { type: 'paragraph' }),
  two_empty_paragraphs: doc(para(t('x')), { type: 'paragraph' }, { type: 'paragraph' }, para(t('y'))),
  two_code_blocks: doc({ type: 'codeBlock', content: [t('a')] }, { type: 'codeBlock', attrs: { language: 'sh' }, content: [t('b')] }),
  code_with_fence: doc({ type: 'codeBlock', content: [t('```\nx\n```')] }),
  empty_code: doc({ type: 'codeBlock' }),
  blockquote_list: doc({ type: 'blockquote', content: [{ type: 'bulletList', content: [item(para(t('a')))] }] }),
  ordered_in_bullet: doc({ type: 'bulletList', content: [item(para(t('a')), { type: 'orderedList', content: [item(para(t('b')))] })] }),
  item_two_paragraphs: doc({ type: 'bulletList', content: [item(para(t('a')), para(t('b')))] }),
  item_code_block: doc({ type: 'bulletList', content: [item(para(t('a')), { type: 'codeBlock', content: [t('x\ny')] })] }),
  heading_marks: doc({ type: 'heading', attrs: { level: 2 }, content: [t('a', 'bold'), t(' b')] }),
  unknown_block: doc({ type: 'databaseView', attrs: { databaseId: 'x' } }, para(t('y'))),
  hard_break_start: doc(para({ type: 'hardBreak' }, t('x'))),
  text_newline: doc(para(t('a\nb'))),
  trailing_spaces: doc(para(t('a  '))),
  image_no_alt: doc({ type: 'image', attrs: { src: 'https://e.com/i.png' } }),
  ordered_start_zero: doc({ type: 'orderedList', attrs: { start: 0 }, content: [item(para(t('a')))] }),
  ordered_ten: doc({ type: 'orderedList', attrs: { start: 9 }, content: ['a', 'b'].map((x) => item(para(t(x)))) }),
  quote_two_paragraphs: doc({ type: 'blockquote', content: [para(t('a')), para(t('b'))] }),
  heading_levels: doc(...[1, 2, 3, 4, 5, 6].map((level) => ({ type: 'heading', attrs: { level }, content: [t(`h${level}`)] }))),
})

const out = Object.entries(cases).map(([name, markdown]) => {
  const doc = manager.parse(markdown)
  return { name, markdown, doc, plain_text: plainText(doc), markdown_out: manager.serialize(doc) }
})

for (const [name, d] of Object.entries(docCases)) {
  out.push({ name: `doc_${name}`, markdown: null, doc: d, plain_text: plainText(d), markdown_out: manager.serialize(d) })
}

const target = join(dirname(fileURLToPath(import.meta.url)), '../../crates/gpui-editor-core/tests/fixtures/tiptap_markdown.json')
writeFileSync(target, JSON.stringify(out, null, 2) + '\n')
console.log(`${out.length} cas écrits dans ${target}`)
