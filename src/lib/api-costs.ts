/**
 * What X's API would charge for the things a classic third-party client did, from X's own rate
 * card. Twister does none of it: the page in the island is X's site, and nothing here is a call
 * Twister makes. The table is a reference, so the cost of the wrapper's alternative is a number and
 * not a feeling.
 *
 * Read off docs.x.com/x-api/getting-started/pricing and the changelog on the date below. X has
 * repriced twice in 2026 already; when the card moves, this file is the one place to follow it.
 */

export const RATE_CARD_CHECKED = '2026-09-10'

/** The day likes, follows and quote posts left every self-serve tier. */
export const SELF_SERVE_CUT = '2026-04-20'

type Unit = 'resource' | 'request'

type Availability =
  /** On the card, with a price. */
  | 'priced'
  /** Not named on the card; X's write rows run $0.005 to $0.015. */
  | 'unlisted'
  /** Off every self-serve tier since `SELF_SERVE_CUT`; an Enterprise contract or nothing. */
  | 'enterprise'

export interface ClientAction {
  /** What the person did, in the client's words. */
  does: string
  /** The row on X's card it bills under. */
  row: string
  usd: number | null
  unit: Unit | null
  status: Availability
  /** A plausible count for one ordinary day, distinct resources only. */
  perDay: number
  /** Counts what: "posts", "profiles", "messages"… */
  each: string
}

const POST_CREATE_USD = 0.015
const POST_CREATE_WITH_LINK_USD = 0.2

/**
 * A reader's day, as a classic client spent it. X bills reads per resource returned and
 * deduplicates a resource within one UTC day, so these are distinct posts and people, not scrolls;
 * a timeline refreshed ten times that shows the same 300 posts costs 300.
 */
export const CLIENT_ACTIONS: ClientAction[] = [
  {
    does: 'Refresh the home timeline',
    row: 'Posts: Read',
    usd: 0.005,
    unit: 'resource',
    status: 'priced',
    perDay: 300,
    each: 'posts',
  },
  {
    does: 'Open a post and its replies',
    row: 'Posts: Read',
    usd: 0.005,
    unit: 'resource',
    status: 'priced',
    perDay: 60,
    each: 'posts',
  },
  {
    does: 'Open a profile',
    row: 'User: Read',
    usd: 0.01,
    unit: 'resource',
    status: 'priced',
    perDay: 10,
    each: 'profiles',
  },
  {
    does: 'Read what a profile posted',
    row: 'Posts: Read',
    usd: 0.005,
    unit: 'resource',
    status: 'priced',
    perDay: 100,
    each: 'posts',
  },
  {
    does: 'Search',
    row: 'Posts: Read',
    usd: 0.005,
    unit: 'resource',
    status: 'priced',
    perDay: 40,
    each: 'posts',
  },
  {
    does: 'Mentions and notifications',
    row: 'Owned read',
    usd: 0.001,
    unit: 'resource',
    status: 'priced',
    perDay: 30,
    each: 'posts',
  },
  {
    does: 'Someone else’s followers or following',
    row: 'Following/Followers: Read',
    usd: 0.01,
    unit: 'resource',
    status: 'priced',
    perDay: 40,
    each: 'people',
  },
  {
    does: 'Read direct messages',
    row: 'DM Event: Read',
    usd: 0.01,
    unit: 'resource',
    status: 'priced',
    perDay: 10,
    each: 'messages',
  },
  {
    does: 'Send a direct message',
    row: 'DM Interaction: Create',
    usd: 0.015,
    unit: 'request',
    status: 'priced',
    perDay: 3,
    each: 'messages',
  },
  {
    does: 'Post or reply',
    row: 'Post: Create',
    usd: POST_CREATE_USD,
    unit: 'request',
    status: 'priced',
    perDay: 4,
    each: 'posts',
  },
  {
    does: 'Post with a link in it',
    row: 'Post: Create (with URL)',
    usd: POST_CREATE_WITH_LINK_USD,
    unit: 'request',
    status: 'priced',
    perDay: 1,
    each: 'posts',
  },
  {
    does: 'Bookmark',
    row: 'Bookmark',
    usd: 0.005,
    unit: 'request',
    status: 'priced',
    perDay: 2,
    each: 'posts',
  },
  {
    does: 'Like',
    row: 'Enterprise only',
    usd: null,
    unit: null,
    status: 'enterprise',
    perDay: 10,
    each: 'posts',
  },
  {
    does: 'Quote',
    row: 'Enterprise only',
    usd: null,
    unit: null,
    status: 'enterprise',
    perDay: 1,
    each: 'posts',
  },
  {
    does: 'Follow or unfollow',
    row: 'Enterprise only',
    usd: null,
    unit: null,
    status: 'enterprise',
    perDay: 1,
    each: 'people',
  },
  {
    does: 'Repost or undo a repost',
    row: 'Not named on the card',
    usd: null,
    unit: null,
    status: 'unlisted',
    perDay: 3,
    each: 'posts',
  },
  {
    does: 'Delete a post',
    row: 'Not named on the card',
    usd: null,
    unit: null,
    status: 'unlisted',
    perDay: 1,
    each: 'posts',
  },
]

export interface DayCost {
  /** The priced rows, summed. */
  usd: number
  /** What the sum leaves out, because X sells it to enterprises or does not name it. */
  unpriced: string[]
}

export function dayCost(actions: ClientAction[] = CLIENT_ACTIONS): DayCost {
  let total = 0
  const unpriced: string[] = []
  for (const action of actions) {
    if (action.usd === null) unpriced.push(action.does)
    else total += action.usd * action.perDay
  }
  return { usd: total, unpriced }
}

/** What posting one prepared part would cost through the API. */
export function partCost(hasLink: boolean): number {
  return hasLink ? POST_CREATE_WITH_LINK_USD : POST_CREATE_USD
}

export function usd(amount: number): string {
  // Three decimals under a cent — $0.005 is a real price on this card — and
  // cents from there.
  const digits = amount > 0 && amount < 0.01 ? 3 : 2
  return `$${amount.toFixed(digits)}`
}
