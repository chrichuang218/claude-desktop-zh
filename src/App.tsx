import { invoke } from '@tauri-apps/api/core'
import {
  AlertCircle,
  CheckCircle2,
  Clipboard,
  Loader2,
  Play,
  RefreshCw,
  RotateCcw,
  Trash2,
  Wrench,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import './App.css'

type LauncherState = 'loading' | 'ready' | 'missing' | 'update' | 'repair'
type Tone = 'good' | 'warn' | 'danger'
type LanguageCode = 'zh-CN' | 'zh-TW' | 'zh-HK'
type PatchMode = 'safe' | 'official'
type ActionCommand =
  | 'open_claude'
  | 'check_update'
  | 'install_patch'
  | 'restore_patch'
  | 'set_auto_updates'

type LauncherStatus = {
  state: LauncherState
  installed: boolean
  localized: boolean
  version: string
  launcher_ready: boolean
  shortcut_ready: boolean
  patcher_ready: boolean
  python_ready: boolean
  engine_ready: boolean
  backup_ready: boolean
  language: string
  install_path: string
  engine_path: string
  message: string
}

type ActionResult = {
  ok: boolean
  state: LauncherState
  message: string
  log: string
}

type LiveLog = {
  log: string
  path: string
}

type Activity = {
  id: number
  tone: Tone
  title: string
  summary: string
  detail?: string
}

const EMPTY_STATUS: LauncherStatus = {
  state: 'loading',
  installed: false,
  localized: false,
  version: '',
  launcher_ready: false,
  shortcut_ready: false,
  patcher_ready: false,
  python_ready: false,
  engine_ready: false,
  backup_ready: false,
  language: '未设置',
  install_path: '',
  engine_path: '',
  message: '正在检查本机状态...',
}

const PREVIEW_STATUS: LauncherStatus = {
  state: 'repair',
  installed: true,
  localized: false,
  version: '1.15962.1',
  launcher_ready: true,
  shortcut_ready: true,
  patcher_ready: false,
  python_ready: true,
  engine_ready: false,
  backup_ready: false,
  language: '未设置',
  install_path: 'C:\\Users\\you\\AppData\\Local\\Programs\\Claude\\Claude.exe',
  engine_path: '首次安装时自动下载 javaht/claude-desktop-zh-cn',
  message: '网页预览：真实状态会在桌面应用中显示。',
}

const languageOptions: Array<{ value: LanguageCode; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁体中文（台湾）' },
  { value: 'zh-HK', label: '繁体中文（香港）' },
]

const modeOptions: Array<{ value: PatchMode; label: string; note: string }> = [
  { value: 'safe', label: '稳定模式', note: '优先稳定和可恢复，推荐日常使用' },
  { value: 'official', label: '覆盖更多文本', note: '修改更深层文本，覆盖更完整但风险更高' },
]

const modeDetails: Record<PatchMode, { title: string; body: string; tone: string }> = {
  safe: {
    title: '推荐：稳定模式',
    body: '更适合 WindowsApps 版，优先保证安装、恢复和后续更新稳定。少量很深的菜单或系统文本可能仍是英文。',
    tone: 'safe',
  },
  official: {
    title: '覆盖更多文本',
    body: '会处理更深层的应用包文本，汉化范围更广。对应地，恢复、应用更新和个别工作区能力的风险也更高。',
    tone: 'official',
  },
}

function isTauriApp() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function summarizeDetail(text: string) {
  const raw = text.trim()
  if (!raw) return '操作已完成。'
  if (/cancel|取消|operation was canceled/i.test(raw)) return '管理员授权已取消，操作没有继续。'
  if (/拒绝访问|Access.*denied|permission|权限/i.test(raw)) return '权限不足，请允许管理员授权后重试。'
  if (/正在运行|被占用|文件占用|locked|in use|resource busy|EBUSY|EPERM/i.test(raw)) {
    return 'Claude Desktop 可能正在运行，请关闭 Claude Desktop 后重试。'
  }
  if (/network|timeout|timed out|ECONNRESET|ENOTFOUND|无法连接|请求失败|下载失败/i.test(raw)) {
    return '网络连接异常，请检查网络后重试。'
  }
  if (/下载补丁引擎失败|GitHub 压缩包内容为空|缺少 Windows 安装脚本/i.test(raw)) {
    return '补丁引擎下载失败，请检查网络后重试。'
  }
  if (/未找到 Claude Desktop|No Claude|Claude Desktop.*not/i.test(raw)) {
    return '未找到 Claude Desktop，请先安装官方 Windows 版。'
  }
  if (/另一个安装进程正在运行|mutex/i.test(raw)) return '已有补丁进程正在运行，请等待完成后再试。'
  if (/暂时无法从 winget|无法读取最新版本|查询失败/i.test(raw)) {
    return '暂时无法检查官方版本，不影响本地汉化操作。'
  }
  if (/安装完成|中文补丁已安装/i.test(raw)) return '中文补丁已安装。'
  if (/卸载完成|恢复原样/i.test(raw)) return '已恢复原样。'
  if (/up to date|最新版本|当前已经是最新/i.test(raw)) return '当前已经是最新版本。'
  return raw.split(/\r?\n/).find(Boolean)?.slice(0, 140) || raw.slice(0, 140)
}

function streamsLiveLog(command: ActionCommand | null) {
  return command !== null && command !== 'open_claude'
}

function pollsPatchLog(command: ActionCommand | null) {
  return command !== null && command !== 'open_claude' && command !== 'check_update'
}

function actionStartingLog(command: ActionCommand) {
  if (command === 'check_update') return '正在读取 winget 官方版本信息...'
  return '等待管理员授权或补丁进程启动...'
}

function displayPath(path: string) {
  if (path.startsWith('\\\\?\\UNC\\')) return `\\\\${path.slice('\\\\?\\UNC\\'.length)}`
  if (path.startsWith('\\\\?\\')) return path.slice('\\\\?\\'.length)
  return path
}

function statusConclusion(status: LauncherStatus, runningInTauri: boolean) {
  if (!runningInTauri) return '运行桌面应用后会显示真实状态。'
  if (status.state === 'loading') return '正在检测 Claude Desktop 环境。'
  if (!status.installed) return '请先安装官方 Claude Desktop。'
  if (status.localized) return 'Claude Desktop 中文版可以打开。'
  return '需要先安装中文补丁。'
}

function actionHint(status: LauncherStatus, runningInTauri: boolean) {
  if (!runningInTauri) return '网页预览不能操作本机程序，请运行桌面应用。'
  if (status.state === 'loading') return '正在检测本机环境，请稍等。'
  if (!status.installed) return '请先安装官方 Claude Desktop。'
  if (status.localized) return '当前可直接打开。'
  return '需要先安装中文补丁。'
}

function actionHintTone(status: LauncherStatus, runningInTauri: boolean) {
  if (!runningInTauri || !status.installed || !status.localized) return 'warn'
  return 'good'
}

function logBadge(
  busyAction: ActionCommand | null,
  status: LauncherStatus,
  latestActivity?: Activity,
): { label: string; tone: 'done' | 'running' | 'warn' | 'failed' } {
  if (busyAction !== null || status.state === 'loading') return { label: '执行中', tone: 'running' }
  if (latestActivity?.tone === 'danger') return { label: '失败', tone: 'failed' }
  if (latestActivity?.tone === 'warn') return { label: '需注意', tone: 'warn' }
  return { label: '检测完成', tone: 'done' }
}

function statusLog(status: LauncherStatus, runningInTauri: boolean) {
  if (!runningInTauri) {
    return [
      '[启动] 当前是网页预览模式。',
      '[提示] 运行桌面应用后会检测本机 Claude Desktop 和中文补丁状态。',
    ].join('\n')
  }

  const lines = [`[结论] ${statusConclusion(status, runningInTauri)}`]
  lines.push(`[状态] Claude Desktop：${status.installed ? '已安装' : '未找到'}`)
  lines.push(`[状态] 中文补丁：${status.localized ? status.language : '未安装'}`)
  lines.push(`[状态] 补丁引擎：${status.engine_ready ? '已准备' : '首次安装时下载'}`)
  lines.push(`[状态] PowerShell：${status.python_ready ? '可用' : '不可用'}`)
  if (status.install_path) lines.push(`[路径] Claude：${displayPath(status.install_path)}`)
  if (status.engine_path) lines.push(`[路径] 工作目录：${displayPath(status.engine_path)}`)
  if (status.version) lines.push(`[版本] Claude Desktop：${status.version}`)
  if (status.message && status.message !== statusConclusion(status, runningInTauri)) {
    lines.push(`[详情] ${status.message}`)
  }
  return lines.join('\n')
}

function App() {
  const [status, setStatus] = useState<LauncherStatus>(EMPTY_STATUS)
  const [language, setLanguage] = useState<LanguageCode>('zh-CN')
  const [patchMode, setPatchMode] = useState<PatchMode>('safe')
  const [busyAction, setBusyAction] = useState<ActionCommand | null>(null)
  const [activities, setActivities] = useState<Activity[]>([])
  const [liveLog, setLiveLog] = useState('')
  const [liveLogPath, setLiveLogPath] = useState('')
  const [liveLogTitle, setLiveLogTitle] = useState('运行日志')
  const [logCleared, setLogCleared] = useState(false)
  const [showRestoreConfirm, setShowRestoreConfirm] = useState(false)
  const nextIdRef = useRef(1)
  const didInitialRefreshRef = useRef(false)
  const liveLogRef = useRef<HTMLPreElement>(null)
  const runningInTauri = isTauriApp()

  const busy = busyAction !== null || status.state === 'loading'
  const canOpen = status.installed && status.localized
  const displayLog = liveLog || (logCleared ? '' : statusLog(status, runningInTauri))

  const addActivity = useCallback((activity: Omit<Activity, 'id'>) => {
    const id = nextIdRef.current
    nextIdRef.current += 1
    setActivities((items) => [{ ...activity, id }, ...items].slice(0, 5))
  }, [])

  const fetchLiveLog = useCallback(async () => {
    if (!runningInTauri) return
    const nextLog = await invoke<LiveLog>('get_live_log')
    setLiveLog(nextLog.log)
    setLiveLogPath(nextLog.path)
  }, [runningInTauri])

  const refreshStatus = useCallback(
    async (silent = false) => {
      if (!runningInTauri) {
        setStatus(PREVIEW_STATUS)
        if (!silent) {
          addActivity({
            tone: 'warn',
            title: '网页预览',
            summary: '网页里不能操作本机程序；运行桌面应用后才是真实状态。',
          })
        }
        return
      }

      try {
        const nextStatus = await invoke<LauncherStatus>('get_status')
        setStatus(nextStatus)
        if (!silent) setLogCleared(false)
        if (nextStatus.language === 'zh-CN' || nextStatus.language === 'zh-TW' || nextStatus.language === 'zh-HK') {
          setLanguage(nextStatus.language)
        }
        if (!silent) {
          addActivity({
            tone: nextStatus.state === 'ready' ? 'good' : 'warn',
            title: '检查完成',
            summary: nextStatus.message,
          })
        }
      } catch (error) {
        addActivity({
          tone: 'danger',
          title: '状态检查失败',
          summary: summarizeDetail(String(error)),
          detail: String(error),
        })
      }
    },
    [addActivity, runningInTauri],
  )

  useEffect(() => {
    if (didInitialRefreshRef.current) return
    didInitialRefreshRef.current = true
    refreshStatus()
  }, [refreshStatus])

  useEffect(() => {
    if (!pollsPatchLog(busyAction)) return
    fetchLiveLog()
    const timer = window.setInterval(() => {
      fetchLiveLog()
    }, 1000)
    return () => window.clearInterval(timer)
  }, [busyAction, fetchLiveLog])

  useEffect(() => {
    const logElement = liveLogRef.current
    if (!logElement) return
    logElement.scrollTop = logElement.scrollHeight
  }, [displayLog])

  async function runAction(command: ActionCommand, actionName: string, payload?: Record<string, unknown>) {
    if (!runningInTauri) {
      addActivity({
        tone: 'warn',
        title: `${actionName}没有执行`,
        summary: '当前是网页预览，不能操作本机 Claude Desktop。',
      })
      return
    }

    const showsLiveLog = streamsLiveLog(command)
    if (showsLiveLog) {
      setLiveLogTitle(`${actionName}日志`)
      setLiveLog(actionStartingLog(command))
      setLiveLogPath('')
      setLogCleared(false)
    }

    setBusyAction(command)
    try {
      const result = await invoke<ActionResult>(command, payload)
      if (command === 'check_update') {
        setLiveLog(result.log || result.message)
      } else if (showsLiveLog) {
        await fetchLiveLog().catch(() => undefined)
        if (!result.log) {
          setLiveLog(result.message)
        }
      }
      setStatus((prev) => ({ ...prev, state: result.state, message: result.message }))
      const summarySource = result.ok ? result.message : result.log || result.message
      const title = !result.ok && command === 'check_update' ? '暂时无法检查更新' : result.ok ? `${actionName}完成` : `${actionName}失败`
      addActivity({
        tone: result.ok ? 'good' : command === 'check_update' ? 'warn' : 'danger',
        title,
        summary: summarizeDetail(summarySource),
        detail: result.log || result.message,
      })
      await refreshStatus(true)
    } catch (error) {
      if (command === 'check_update') {
        setLiveLog(String(error))
      } else if (showsLiveLog) {
        await fetchLiveLog().catch(() => undefined)
      }
      addActivity({
        tone: 'danger',
        title: `${actionName}失败`,
        summary: summarizeDetail(String(error)),
        detail: String(error),
      })
    } finally {
      setBusyAction(null)
    }
  }

  function installPatch() {
    runAction('install_patch', '安装中文补丁', { language, patchMode })
  }

  function restoreOriginal() {
    setShowRestoreConfirm(true)
  }

  function confirmRestoreOriginal() {
    setShowRestoreConfirm(false)
    runAction('restore_patch', '恢复原样')
  }

  function runPrimaryAction() {
    if (canOpen) {
      runAction('open_claude', '打开')
      return
    }
    installPatch()
  }

  async function copyDiagnostics() {
    const statusText = [
      `状态：${status.message}`,
      `Claude：${status.install_path ? displayPath(status.install_path) : '未找到'}`,
      `版本：${status.version || '未检测到'}`,
      `语言：${status.language}`,
      `补丁引擎：${status.engine_path ? displayPath(status.engine_path) : '未准备'}`,
      liveLog ? `\n${liveLogTitle}\n${liveLog}` : '',
      '',
      ...activities.map((item) => `${item.title}\n${item.summary}\n${item.detail ?? ''}`.trim()),
    ].join('\n')
    await navigator.clipboard?.writeText(statusText)
    addActivity({
      tone: 'good',
      title: '已复制诊断信息',
      summary: '可以把它发给开发者排查问题。',
    })
  }

  async function copyRunLog() {
    await navigator.clipboard?.writeText(displayLog)
    addActivity({
      tone: 'good',
      title: '已复制运行日志',
      summary: '运行日志已复制到剪贴板。',
    })
  }

  function clearRunLog() {
    setLiveLog('')
    setLiveLogPath('')
    setLiveLogTitle('运行日志')
    setLogCleared(true)
  }

  const checks = useMemo(
    () => [
      {
        label: 'Claude Desktop',
        ok: status.installed,
        value: status.installed ? '已安装' : '未找到',
      },
      {
        label: '中文补丁',
        ok: status.localized,
        value: status.localized ? status.language : '未安装',
      },
      {
        label: '补丁引擎',
        ok: status.engine_ready,
        value: status.engine_ready ? '已准备' : '首次安装时下载',
      },
      {
        label: 'PowerShell',
        ok: status.python_ready,
        value: status.python_ready ? '可用' : '不可用',
      },
    ],
    [status],
  )

  const primaryLabel = canOpen ? '打开 Claude' : '安装中文补丁'
  const primaryIcon = busyAction ? <Loader2 className="spin" size={18} /> : canOpen ? <Play size={18} /> : <Wrench size={18} />
  const busyShowsLiveLog = pollsPatchLog(busyAction)
  const primaryHint = actionHint(status, runningInTauri)
  const primaryHintTone = actionHintTone(status, runningInTauri)
  const currentLogBadge = logBadge(busyAction, status, activities[0])

  return (
    <main className="app-shell">
      <header className="top-bar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <div>
            <strong>Claude Desktop</strong>
            <span>中文助手</span>
          </div>
        </div>
        <button className="ghost-button" type="button" onClick={() => refreshStatus()} disabled={busyAction !== null}>
          <RotateCcw size={16} />
          刷新
        </button>
      </header>

      <section className="workspace">
        <section className="main-panel" aria-live="polite">
          <div className="status-hero">
            <span className={`status-pill ${status.state}`}>{status.localized ? '已汉化' : status.installed ? '待安装' : '未找到'}</span>
            <h1>{status.localized ? 'Claude 中文版可以打开' : '安装中文补丁'}</h1>
            <p>{status.message}</p>
          </div>

          <div className="control-grid">
            <label className="field">
              <span>语言</span>
              <select value={language} onChange={(event) => setLanguage(event.target.value as LanguageCode)} disabled={busy}>
                {languageOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>

            <div className="field">
              <span>安装模式</span>
              <div className="segmented" role="group" aria-label="安装模式">
                {modeOptions.map((option) => (
                  <button
                    className={patchMode === option.value ? 'active' : ''}
                    key={option.value}
                    type="button"
                    disabled={busy}
                    onClick={() => setPatchMode(option.value)}
                    title={option.note}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className={`mode-detail ${modeDetails[patchMode].tone}`}>
            <strong>{modeDetails[patchMode].title}</strong>
            <span>{modeDetails[patchMode].body}</span>
          </div>

          <div className="action-row">
            <button className="primary-button" type="button" onClick={runPrimaryAction} disabled={busy || !runningInTauri || !status.installed}>
              {primaryIcon}
              {busyAction ? '处理中...' : primaryLabel}
            </button>
            <div className="secondary-actions">
              <button
                className="outline-button"
                type="button"
                onClick={() => runAction('check_update', '检查更新')}
                disabled={busyAction !== null || !runningInTauri || !status.installed}
              >
                <RefreshCw size={17} />
                检查更新
              </button>
              <button
                className="outline-button"
                type="button"
                onClick={restoreOriginal}
                disabled={busyAction !== null || !runningInTauri || !status.installed}
              >
                恢复原样
              </button>
            </div>
          </div>
          <p className={`action-hint ${primaryHintTone}`}>{primaryHint}</p>

          <section className="live-log-panel" aria-live="polite">
            <div className="live-log-title">
              <div>
                <div className="live-log-heading">
                  <h2>{liveLogTitle}</h2>
                  <span className={`log-state-badge ${currentLogBadge.tone}`}>{currentLogBadge.label}</span>
                </div>
                <span className="live-log-subtitle">{busyShowsLiveLog ? '实时刷新中' : liveLog ? '最后一次执行输出' : '启动检测结果'}</span>
              </div>
              <div className="live-log-tools">
                <button className="icon-button" type="button" onClick={copyRunLog} disabled={!displayLog}>
                  <Clipboard size={14} />
                  复制
                </button>
                <button className="icon-button" type="button" onClick={clearRunLog} disabled={!displayLog}>
                  <Trash2 size={14} />
                  清空
                </button>
              </div>
            </div>
            {liveLogPath ? <code className="log-path-line">{liveLogPath}</code> : null}
            <pre ref={liveLogRef}>{displayLog || '暂无日志。点击刷新或执行操作后会在这里显示输出。'}</pre>
          </section>
        </section>

        <aside className="side-panel">
          <section className="card">
            <div className="section-title">
              <h2>当前状态</h2>
            </div>
            <div className="status-list">
              {checks.map((item) => (
                <div className={`status-row ${item.ok ? 'ok' : 'bad'}`} key={item.label}>
                  {item.ok ? <CheckCircle2 size={17} /> : <AlertCircle size={17} />}
                  <span>{item.label}</span>
                  <strong>{status.state === 'loading' ? '检查中' : item.value}</strong>
                </div>
              ))}
            </div>
          </section>

          <section className="card advanced-card">
            <div className="section-title">
              <h2>更新设置</h2>
            </div>
            <p className="card-note">不确定时保持默认即可。</p>
            <div className="compact-actions">
              <button type="button" onClick={() => runAction('set_auto_updates', '禁止自动更新', { enabled: false })} disabled={busyAction !== null || !runningInTauri}>
                禁止自动更新
              </button>
              <button type="button" onClick={() => runAction('set_auto_updates', '允许自动更新', { enabled: true })} disabled={busyAction !== null || !runningInTauri}>
                允许自动更新
              </button>
            </div>
          </section>

          <section className="card log-card">
            <div className="section-title">
              <h2>诊断</h2>
              <button className="copy-button" type="button" onClick={copyDiagnostics} disabled={activities.length === 0}>
                <Clipboard size={15} />
                复制
              </button>
            </div>
            {activities.length === 0 ? (
              <p className="empty-log">暂无操作记录。</p>
            ) : (
              <ol>
                {activities.map((item) => (
                  <li className={item.tone} key={item.id}>
                    {item.tone === 'good' ? <CheckCircle2 size={17} /> : <AlertCircle size={17} />}
                    <div>
                      <strong>{item.title}</strong>
                      <span>{item.summary}</span>
                      {item.detail && item.detail !== item.summary ? (
                        <details>
                          <summary>技术细节</summary>
                          <pre>{item.detail}</pre>
                        </details>
                      ) : null}
                    </div>
                  </li>
                ))}
              </ol>
            )}
          </section>
        </aside>
      </section>

      {showRestoreConfirm ? (
        <div className="modal-backdrop" role="presentation" onMouseDown={() => setShowRestoreConfirm(false)}>
          <section
            className="confirm-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="restore-confirm-title"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="confirm-icon" aria-hidden="true">
              <RotateCcw size={18} />
            </div>
            <div className="confirm-content">
              <h2 id="restore-confirm-title">恢复原样？</h2>
              <p>这会撤销中文补丁，不会删除 Claude 数据。恢复前请先关闭 Claude Desktop。</p>
            </div>
            <div className="confirm-actions">
              <button className="outline-button" type="button" onClick={() => setShowRestoreConfirm(false)}>
                取消
              </button>
              <button className="primary-button danger-action" type="button" onClick={confirmRestoreOriginal}>
                确认恢复
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </main>
  )
}

export default App
