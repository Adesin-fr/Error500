// Formes des props envoyées par le serveur Rust (voir src/web/*.rs).

export interface AuthUser {
  id: number
  name: string
  email: string
  is_admin: boolean
}

// `type` (et non `interface`) : Inertia exige une signature d'index sur les props de page.
export type SharedProps = {
  auth: { user: AuthUser | null }
}

export interface ProjectRef {
  id: number
  name: string
  slug: string
}

export type ProjectProps = SharedProps & {
  project: ProjectRef
  can_manage: boolean
}

export type IssueStatus = 'unresolved' | 'resolved' | 'ignored'

export interface Issue {
  id: number
  title: string
  level: string
  status: IssueStatus
  first_seen: string
  last_seen: string
  event_count: number
}

export interface Location {
  filename: string
  lineno: number | null
  colno: number | null
}

export interface Frame {
  filename: string | null
  function: string | null
  lineno: number | null
  colno: number | null
  in_app: boolean
  pre_context: string[]
  context_line: string | null
  post_context: string[]
  resolved: boolean
  minified: Location | null
}

export interface ExceptionView {
  type: string | null
  value: string | null
  frames: Frame[]
}

export interface Tag {
  key: string
  value: string
}

export interface Breadcrumb {
  timestamp?: number | string
  type?: string
  category?: string
  level?: string
  message?: string
  data?: unknown
}

export interface EventDetail {
  event_id: string
  occurred_at: string
  release: string | null
  environment: string | null
  platform: string | null
  message: string | null
  user: { id?: string; email?: string; username?: string; ip_address?: string }
  request: { url?: string; method?: string; query_string?: string }
  tags: Tag[]
  breadcrumbs: Breadcrumb[]
  contexts: Record<string, { name?: string; version?: string; family?: string; model?: string; brand?: string }>
}

export interface EventSummary {
  event_id: string
  occurred_at: string
  release: string | null
  environment: string | null
}

export interface Replay {
  replay_id: string
  first_seen: string
  last_seen: string
  duration_seconds: number
  segment_count: number
  urls: string[]
  user_label: string | null
  browser: string | null
  os: string | null
  release_name: string | null
  environment: string | null
  error_ids: string[]
  /** Un segment a été reçu dans la dernière minute : la session est en cours. */
  live: boolean
}

/** Utilisateur ayant au moins une session, pour le filtre de la liste des replays. */
export interface ReplayUser {
  label: string
  count: number
}

export interface LinkedError {
  event_id: string
  issue_id: number
  title: string
}

export interface LogEntry {
  id: number
  occurred_at: string
  level: string
  body: string
  trace_id: string | null
  attributes: Record<string, { value: unknown; type?: string } | unknown> | null
}

export interface Member {
  user_id: number
  name: string
  email: string
  role: 'owner' | 'member'
  notify: boolean
}

export interface SourceMapEntry {
  id: number
  release: string | null
  name: string
  debug_id: string | null
  size_bytes: number
}

export interface ManagedUser {
  id: number
  name: string
  email: string
  is_admin: boolean
  created_at: string
}

export type FormErrors = Partial<Record<string, string>>
