(function initAccountUi(root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.AccountUi = api;
  }
}(typeof globalThis !== 'undefined' ? globalThis : this, function factory() {
  function createAccountUi(deps) {
    const {
      query,
      serviceMeta,
      savedTokenMask,
      escHtml,
      deriveTokenInputValue,
      normalizeAccountToken,
      queuePersistSetup,
      deleteAccount,
      oauthLogin,
      oauthExchangeCode,
      log,
      makeId,
    } = deps;

    const makeAccountId = makeId || ((service) => `${service}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);

    function defaultAccount(service, idx = 0) {
      const base = serviceMeta[service].label;
      return { id: makeAccountId(service), name: `${base} ${idx + 1}`, token: '', hasToken: false };
    }

    function accountFromRow(row) {
      const id = row.dataset.accountId || '';
      const name = row.querySelector('.account-name')?.value?.trim() || '';
      const rawToken = row.querySelector('.account-token')?.value || '';
      const token = normalizeAccountToken({
        rawToken,
        tokenMasked: row.dataset.tokenMasked === '1',
        savedTokenMask,
      });
      const hasToken = row.dataset.hasToken === '1';
      return { id, name, token, hasToken };
    }

    function readAccountsFromDom(service) {
      const list = query(serviceMeta[service].listId);
      const rows = Array.from(list.querySelectorAll('.account-row'));
      return rows.map(accountFromRow);
    }

    function upsertDomTokenState(service, id, hasToken) {
      const list = query(serviceMeta[service].listId);
      const row = Array.from(list.querySelectorAll('.account-row'))
        .find((candidate) => candidate.dataset.accountId === id);
      if (!row) return;
      row.dataset.hasToken = hasToken ? '1' : '0';
      const tokenInput = row.querySelector('.account-token');
      if (tokenInput) {
        tokenInput.placeholder = 'eyJhbG... / sk-...';
        if (hasToken) {
          tokenInput.value = savedTokenMask;
          row.dataset.tokenMasked = '1';
        } else {
          tokenInput.value = '';
          row.dataset.tokenMasked = '0';
        }
      }
    }

    function writeAccountsToDom(service, accounts) {
      const list = query(serviceMeta[service].listId);
      list.innerHTML = '';
      for (const acc of accounts) {
        const row = document.createElement('div');
        row.className = 'account-row';
        row.dataset.accountId = acc.id || makeAccountId(service);
        row.dataset.hasToken = acc.hasToken ? '1' : '0';

        const tokenView = deriveTokenInputValue({
          hasToken: acc.hasToken,
          token: acc.token,
          savedTokenMask,
        });
        row.dataset.tokenMasked = tokenView.tokenMasked ? '1' : '0';

        row.innerHTML = `
          <input class="account-name" type="text" maxlength="256" placeholder="表示名" value="${escHtml(acc.name || '')}">
          <input class="account-token" type="password" maxlength="16384" autocomplete="off" placeholder="eyJhbG... / sk-..." value="${escHtml(tokenView.tokenValue)}">
          <button class="btn-mini btn-oauth-login" type="button" title="OAuth ログイン">🔐 ログイン</button>
          <button class="btn-mini btn-remove-account" type="button">削除</button>
          <span class="oauth-status" data-status=""></span>
        `;

        row.querySelector('.btn-remove-account').addEventListener('click', async () => {
          const removed = accountFromRow(row);
          if (removed.id) {
            try {
              await deleteAccount({ service, id: removed.id });
            } catch (e) {
              const message = e && typeof e.message === 'string' && e.message.trim()
                ? e.message
                : String(e ?? 'unknown error');
              log(`削除失敗: ${serviceMeta[service].label} ${removed.name || removed.id} (${message})`, 'warn');
              return;
            }
          }
          row.remove();
          if (!list.querySelector('.account-row')) addAccountRow(service);
          queuePersistSetup();
        });

        row.querySelector('.btn-oauth-login').addEventListener('click', async () => {
          const acc = accountFromRow(row);
          const statusEl = row.querySelector('.oauth-status');
          const loginBtn = row.querySelector('.btn-oauth-login');
          statusEl.textContent = 'ブラウザを開いています...';
          statusEl.dataset.status = 'pending';
          loginBtn.disabled = true;
          try {
            const result = await oauthLogin({ service, id: acc.id });
            if (result.needsCode) {
              // Claude two-step flow: prompt user for the code
              statusEl.textContent = result.message;
              statusEl.dataset.status = 'pending';
              loginBtn.disabled = false;
              const code = prompt('ブラウザに表示された認証コードを貼り付けてください:');
              if (!code || !code.trim()) {
                statusEl.textContent = 'キャンセルされました';
                statusEl.dataset.status = 'error';
                return;
              }
              loginBtn.disabled = true;
              statusEl.textContent = 'コードを送信中...';
              const exchangeResult = await oauthExchangeCode({ service, id: acc.id, code: code.trim() });
              if (exchangeResult.success) {
                statusEl.textContent = 'ログイン成功';
                statusEl.dataset.status = 'ok';
                row.dataset.hasToken = '1';
                const tokenInput = row.querySelector('.account-token');
                if (tokenInput) { tokenInput.value = savedTokenMask; row.dataset.tokenMasked = '1'; }
                queuePersistSetup();
              } else {
                statusEl.textContent = exchangeResult.message || 'ログイン失敗';
                statusEl.dataset.status = 'error';
              }
            } else if (result.success) {
              statusEl.textContent = 'ログイン成功';
              statusEl.dataset.status = 'ok';
              row.dataset.hasToken = '1';
              const tokenInput = row.querySelector('.account-token');
              if (tokenInput) { tokenInput.value = savedTokenMask; row.dataset.tokenMasked = '1'; }
              queuePersistSetup();
            } else {
              statusEl.textContent = result.message || 'ログイン失敗';
              statusEl.dataset.status = 'error';
            }
          } catch (e) {
            const msg = e && typeof e === 'string' ? e : (e?.message || String(e));
            statusEl.textContent = msg;
            statusEl.dataset.status = 'error';
            log(`OAuth ログインエラー (${serviceMeta[service].label}): ${msg}`, 'warn');
          } finally {
            loginBtn.disabled = false;
          }
        });

        row.querySelector('.account-name').addEventListener('input', queuePersistSetup);

        const tokenInput = row.querySelector('.account-token');
        tokenInput.addEventListener('focus', () => {
          if (row.dataset.tokenMasked === '1' && tokenInput.value === savedTokenMask) {
            setTimeout(() => tokenInput.select(), 0);
          }
        });
        tokenInput.addEventListener('input', () => {
          row.dataset.tokenMasked = '0';
          row.dataset.hasToken = tokenInput.value?.trim() ? '0' : row.dataset.hasToken;
          queuePersistSetup();
        });

        list.appendChild(row);
      }
    }

    function addAccountRow(service, account = null) {
      const existing = readAccountsFromDom(service);
      const next = account || defaultAccount(service, existing.length);
      writeAccountsToDom(service, [...existing, next]);
      queuePersistSetup();
    }

    function collectAccounts() {
      const collected = {};
      for (const service of Object.keys(serviceMeta)) {
        const rows = readAccountsFromDom(service);
        collected[service] = rows.map((acc, idx) => ({
          id: acc.id || makeAccountId(service),
          name: acc.name || `${serviceMeta[service].label} ${idx + 1}`,
          token: acc.token || '',
          hasToken: Boolean(acc.hasToken),
        }));
      }
      return collected;
    }

    return {
      makeAccountId,
      defaultAccount,
      accountFromRow,
      readAccountsFromDom,
      writeAccountsToDom,
      addAccountRow,
      collectAccounts,
      upsertDomTokenState,
    };
  }

  return {
    createAccountUi,
  };
}));
