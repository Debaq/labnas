export interface Note {
  id: string
  title: string
  content: string
  created_by: string
  updated_by: string
  shared_with: string[]
  is_public: boolean
  created_at: string
  updated_at: string
}

export interface ShareLink {
  token: string
  file_name: string
  file_path: string
  url: string
  remaining_minutes: number
}
