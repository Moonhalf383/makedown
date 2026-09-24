if exists('b:current_syntax')
  finish
endif

runtime! syntax/markdown.vim
unlet! b:current_syntax

syntax region mkdTemplateComment start=/{#/ end=/#}/ keepend containedin=ALL contains=NONE
syntax region mkdTemplateExpression start=/{{-\?/ end=/-\?}}/ keepend containedin=ALL contains=mkdTemplateVariable,mkdTemplateFilter,mkdTemplateString
syntax region mkdTemplateStatement start=/{%-\?/ end=/-\?%}/ keepend containedin=ALL contains=mkdTemplateKeyword,mkdTemplateVariable,mkdTemplateFilter,mkdTemplateString
syntax keyword mkdTemplateKeyword for endfor if elif else endif set raw endraw namespace contained
syntax match mkdTemplateVariable /\<\%(plan\|stage\|target\|loop\)\>\%([.]\h\w*\)*/ contained
syntax match mkdTemplateFilter /|\s*\h\w*/ contained
syntax region mkdTemplateString start=/'/ skip=/\\'/ end=/'/ contained
syntax region mkdTemplateString start=/"/ skip=/\\"/ end=/"/ contained

highlight default link mkdTemplateComment Comment
highlight default link mkdTemplateExpression Delimiter
highlight default link mkdTemplateStatement PreProc
highlight default link mkdTemplateKeyword Statement
highlight default link mkdTemplateVariable Identifier
highlight default link mkdTemplateFilter Function
highlight default link mkdTemplateString String

let b:current_syntax = 'mkd_template'
