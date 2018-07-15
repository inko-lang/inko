# frozen_string_literal: true

module Inkoc
  # The version of the Inko language to target.
  LANGUAGE_VERSION =
    File.read(File.expand_path('../../LANGUAGE_VERSION', __dir__)).strip
end
