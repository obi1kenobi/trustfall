/**
 *  Copyright (c) 2022 GraphQL Contributors.
 *
 *  This source code is licensed under the MIT license found in the
 *  LICENSE file.
 *
 *  This code has been slightly adapted to change the styling of elements.
 *  Original code is available here:
 *  Adapted from https://github.com/graphql/graphiql
 */
import ClearIcon from '@mui/icons-material/Clear';
import { TextField } from '@mui/material';
import IconButton from '@mui/material/IconButton';
import SearchIcon from '@mui/icons-material/Search';
import InputAdornment from '@mui/material/InputAdornment';
import React, { ChangeEventHandler } from 'react';

function debounce<F extends (...args: any[]) => any>(duration: number, fn: F) {
  let timeout: number | null;
  return function (this: any, ...args: Parameters<F>) {
    if (timeout) {
      window.clearTimeout(timeout);
    }
    timeout = window.setTimeout(() => {
      timeout = null;
      fn.apply(this, args);
    }, duration);
  };
}

type OnSearchFn = (value: string) => void;

type SearchBoxProps = {
  value?: string;
  placeholder: string;
  onSearch: OnSearchFn;
};

type SearchBoxState = {
  value: string;
};

export default class SearchBox extends React.Component<SearchBoxProps, SearchBoxState> {
  debouncedOnSearch: OnSearchFn;

  constructor(props: SearchBoxProps) {
    super(props);
    this.state = { value: props.value || '' };
    this.debouncedOnSearch = debounce(200, this.props.onSearch);
  }

  render() {
    return (
      <TextField
        value={this.state.value}
        onChange={this.handleChange}
        label={this.props.placeholder}
        type="text"
        aria-label={this.props.placeholder}
        size="small"
        InputProps={{
          startAdornment: (
            <InputAdornment position="start">
              <SearchIcon />
            </InputAdornment>
          ),
          endAdornment: (
            <InputAdornment position="end">
              <IconButton
                aria-label="Clear search input"
                onClick={this.handleClear}
                disabled={!this.state.value}
              >
                <ClearIcon />
              </IconButton>
            </InputAdornment>
          ),
        }}
        fullWidth
      />
    );
  }

  handleChange: ChangeEventHandler<HTMLInputElement> = (event) => {
    const value = event.currentTarget.value;
    this.setState({ value });
    this.debouncedOnSearch(value);
  };

  handleClear = () => {
    this.setState({ value: '' });
    this.props.onSearch('');
  };
}
