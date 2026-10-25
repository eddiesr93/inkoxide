/**
 * The components of the lifecycle tests that are alive: each counts itself when it is created and
 * when it is released (unmounted, destroyed).
 */
export const live = { created: 0, released: 0 };
