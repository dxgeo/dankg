// Prepended to the generated .typ, so every `#set` here applies
// document-wide. dankg writes none of this itself: the template is the
// author's own file, and typesetting choices belong in it.

// A bare `[[#heading]]` reference emits `@sec:...`, and Typst cannot
// reference a heading it has not numbered (decision 67). This one line
// is what makes those compile.
#set heading(numbering: "1.")

// Typst puts every figure's caption below its content by default. The
// conventional split is captions above tables, below images.
#show figure.where(kind: table): set figure.caption(position: top)

#set page(margin: 2.2cm)
#set text(size: 10.5pt)
