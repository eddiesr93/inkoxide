import { Component, CUSTOM_ELEMENTS_SCHEMA } from '@angular/core';

/** A template whose classes are only in its own file: the plugin's scan reads them there. */
@Component({ selector: 'app-card', schemas: [CUSTOM_ELEMENTS_SCHEMA], templateUrl: './card.html' })
export class Card {}
