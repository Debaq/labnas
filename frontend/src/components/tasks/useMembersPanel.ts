import { useState } from 'react'

export function useMembersPanel() {
  const [showMembers, setShowMembers] = useState(false)
  const [newMember, setNewMember] = useState('')
  const [newTagUser, setNewTagUser] = useState('')
  const [newTagValue, setNewTagValue] = useState('')
  return { showMembers, setShowMembers, newMember, setNewMember, newTagUser, setNewTagUser, newTagValue, setNewTagValue }
}

export type MembersPanelState = ReturnType<typeof useMembersPanel>
