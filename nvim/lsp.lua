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

-- Noice 用独立浮窗绘制菜单时，将文档放在实际菜单的侧边而非其后面。
local function place_doc_beside_noice(preview)
  local noice = package.loaded["noice.ui.popupmenu.nui"]
  local menu = noice and noice.menu
  local menu_win = menu and menu.winid
  if not menu_win or not vim.api.nvim_win_is_valid(menu_win) then
    return
  end
  local pos = vim.api.nvim_win_get_position(menu_win)
  local width = vim.api.nvim_win_get_width(menu_win)
  local config = vim.api.nvim_win_get_config(preview)
  local right = pos[2] + width + 1
  local left = pos[2] - config.width - 1
  local col = right + config.width <= vim.o.columns and right or left
  if col < 0 then
    return
  end
  config.relative = "editor"
  config.row = pos[1]
  config.col = col
  config.zindex = math.max(config.zindex or 50, (vim.api.nvim_win_get_config(menu_win).zindex or 50) + 1)
  vim.api.nvim_win_set_config(preview, config)
end

-- 为 mkd 客户端自动开启补全；Neovim 0.11+ 不再默认弹出 LSP 候选。
local doc_group = vim.api.nvim_create_augroup("mkd-completion-doc", { clear = true })
vim.api.nvim_create_autocmd("LspAttach", {
  callback = function(event)
    local client = vim.lsp.get_client_by_id(event.data.client_id)
    if not client or client.name ~= "mkd" then
      return
    end
    vim.bo[event.buf].completeopt = "menuone,noselect,popup"
    vim.lsp.completion.enable(true, client.id, event.buf, { autotrigger = true })
    -- 有些配置虽保留了 info 与 popup，却不会自动创建预览浮窗。
    vim.api.nvim_clear_autocmds({ group = doc_group, buffer = event.buf })
    vim.api.nvim_create_autocmd("CompleteChanged", {
      group = doc_group,
      buffer = event.buf,
      callback = function()
        local item = vim.v.event.completed_item or {}
        if vim.tbl_get(item, "user_data", "nvim", "lsp", "client_id") ~= client.id then
          return
        end
        local selected = vim.fn.complete_info({ "selected" }).selected
        if selected >= 0 and item.info and item.info ~= "" then
          local window = vim.api.nvim__complete_set(selected, { info = item.info })
          if window and window.winid and vim.api.nvim_win_is_valid(window.winid) then
            place_doc_beside_noice(window.winid)
          end
        end
      end,
    })
  end,
})
