local source = debug.getinfo(1, "S").source:sub(2)
local project = vim.fn.fnamemodify(source, ":h:h")

vim.opt.runtimepath:prepend(project .. "/nvim")
vim.filetype.add({ extension = { mf = "mf" } })

vim.lsp.config("mkd", {
  cmd = { project .. "/target/debug/mkd-lsp" },
  filetypes = { "mf" },
  root_markers = { "main.mf", ".git" },
  single_file_support = true,
})
vim.lsp.enable("mkd")
