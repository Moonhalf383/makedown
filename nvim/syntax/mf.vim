if exists('b:current_syntax')
  finish
endif

syntax match mfSeparator /^---\s*$/
syntax match mfTarget /^#\+\s*.*/ contains=mfTargetMarker
syntax match mfTargetMarker /^#\+/ contained
syntax match mfDirective /^>\s\+.*/ contains=mfDirectiveMarker,mfAnchor,mfAlias
syntax match mfDirective /^>\s*$/ contains=mfDirectiveMarker
syntax match mfDirectiveMarker /^>/ contained
syntax match mfAnchor /\<\%(crate\|self\|super\)\ze::/ contained
syntax match mfAlias /\<as\>\ze\s\+\S/ contained
syntax match mfSpecification /^-\s\+.*/ contains=mfSpecificationMarker
syntax match mfSpecification /^-\s*$/ contains=mfSpecificationMarker
syntax match mfSpecificationMarker /^-/ contained

highlight default link mfSeparator Delimiter
highlight default link mfTarget Title
highlight default link mfTargetMarker Special
highlight default link mfDirective Identifier
highlight default link mfDirectiveMarker Operator
highlight default link mfAnchor Keyword
highlight default link mfAlias Keyword
highlight default link mfSpecification String
highlight default link mfSpecificationMarker Operator

let b:current_syntax = 'mf'
