# Avatar

Avatars identify speakers and Saved People with initials on a soft speaker colour and a 1.5px ring in the full colour.

- New speakers take the next unused `speaker-1` … `speaker-8` in a meeting.
- A Saved Person picks a colour in their profile; it sticks across every meeting and wins over the auto colour.
- Initials are always `ink` on `speaker-N-soft` (passes 4.5:1 in both themes). Photos are optional and replace the initials, keeping the ring.
- Stacks overlap by 6px with a 2px `bg-canvas` outline and fan out 4px per avatar on hover with `--spring-snappy`.
