# frozen_string_literal: true

module Inkoc
  module TypePass
    def initialize(compiler, mod)
      @module = mod
      @state = compiler.state
      @constant_resolver = ConstantResolver.new(diagnostics)
    end

    def diagnostics
      @state.diagnostics
    end

    def typedb
      @state.typedb
    end

    def run(ast)
      locals = ast.locals
      scope = TypeScope
        .new(@module.type, @module.body.type, @module, locals: locals)

      on_module_body(ast, scope)

      [ast]
    end

    def on_module_body(node, scope)
      define_type(node, scope)
      nil
    end

    def on_body(node, scope)
      define_types(node.expressions, scope)
      nil
    end

    def on_constant(node, scope)
      @constant_resolver.resolve(node, scope)
    end

    def on_never_type(node, _)
      wrap_optional_type(node, TypeSystem::Never.new)
    end

    def on_any_type(node, _)
      wrap_optional_type(node, TypeSystem::Any.singleton)
    end

    def on_self_type_with_late_binding(node, _)
      wrap_optional_type(node, TypeSystem::SelfType.new)
    end

    def on_self_type(node, scope)
      self_type = scope.self_type

      # When "Self" translates to a generic type, e.g. Array!(T), we want to
      # return a type in the form of `Array!(T -> T)`, and not just `Array`.
      # This ensures that any arguments passed to a method returning "Self" can
      # properly initialise the type.
      type_arguments =
        self_type.generic_type? ? self_type.type_parameters.to_a : []

      wrap_optional_type(node, self_type.new_instance(type_arguments))
    end

    def on_type_name(node, scope)
      type = define_type(node.name, scope)

      return type if type.error?
      return wrap_optional_type(node, type) unless type.generic_type?

      # When our type is a generic type we need to initialise it according to
      # the passed type parameters.
      type_arguments = []

      node.type_parameters.zip(type.type_parameters) do |param_node, param|
        param_instance = define_type_instance(param_node, scope)

        if param && !param_instance.type_compatible?(param, @state)
          return diagnostics
            .type_error(param, param_instance, param_node.location)
        end

        type_arguments << param_instance
      end

      num_given = type_arguments.length
      num_expected = type.type_parameters.length

      if num_given != num_expected
        return diagnostics.type_parameter_count_error(
          num_given,
          num_expected,
          node.location
        )
      end

      # Simply referencing a constant should not lead to it being initialised,
      # unless there are any type parameters to initialise.
      wrap_optional_type(
        node,
        type.new_instance_for_reference(type_arguments)
      )
    end

    def define_type(node, scope, *extra)
      type = process_node(node, scope, *extra)

      node.type ||= type if type
    end

    def define_types(nodes, scope, *extra)
      nodes.map { |n| define_type(n, scope, *extra) }
    end

    def define_type_instance(node, scope, *extra)
      type = define_type(node, scope, *extra)

      if type && !type.type_instance?
        type = type.new_instance
        node.type = type
      end

      type
    end

    def wrap_optional_type(node, type)
      if node.optional?
        TypeSystem::Optional.wrap(type)
      else
        type
      end
    end

    def store_type(type, scope, location)
      scope.self_type.define_attribute(type.name, type)

      store_type_as_global(type.name, type, scope, location)

      type
    end

    def store_type_as_global(name, type, scope, location)
      if Config::RESERVED_CONSTANTS.include?(name)
        diagnostics.redefine_reserved_constant_error(name, location)
      elsif scope.module_scope?
        @module.globals.define(name, type)
      end
    end

    def scope_for_object_body(node)
      TypeScope
        .new(node.type, node.block_type, @module, locals: node.body.locals)
    end

    def define_required_traits(node, trait, scope)
      node.required_traits.each do |req_node|
        req = define_type_instance(req_node, scope)

        trait.add_required_trait(req) unless req.error?
      end
    end

    def new_object_type
      @module.lookup_object_type&.new_instance
    end

    def on_raw_instruction(node, scope)
      callback = node.raw_instruction_visitor_method

      define_types(node.arguments, scope)

      if respond_to?(callback)
        public_send(callback, node, scope)
      else
        diagnostics.unknown_raw_instruction_error(node.name, node.location)

        TypeSystem::Error.new
      end
    end

    def on_raw_set_attribute(node, *)
      node.arguments.fetch(2).type
    end

    def on_raw_get_attribute(node, *)
      object = node.arguments.fetch(0).type
      name = node.arguments.fetch(1)

      if name.string?
        object.lookup_attribute(name.value).type
      else
        new_object_type
      end
    end
    alias on_raw_get_attribute_in_self on_raw_get_attribute

    def on_raw_allocate(node, *)
      if (proto = node.arguments[0]&.type)
        proto = proto.type if proto.optional?

        proto.new_instance
      else
        typedb.new_empty_object
      end
    end

    def on_raw_object_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_copy_blocks(*)
      TypeSystem::Never.new
    end

    def on_raw_integer_to_string(*)
      typedb.string_type.new_instance
    end

    def on_raw_integer_to_float(*)
      typedb.float_type.new_instance
    end

    def on_raw_integer_add(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_div(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_mul(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_sub(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_mod(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_bitwise_and(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_bitwise_or(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_bitwise_xor(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_shift_left(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_shift_right(*)
      typedb.integer_type.new_instance
    end

    def on_raw_integer_smaller(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_greater(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_greater_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_integer_smaller_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_to_string(*)
      typedb.string_type.new_instance
    end

    def on_raw_float_to_integer(*)
      typedb.integer_type.new_instance
    end

    def on_raw_float_add(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_div(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_mul(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_sub(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_mod(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_smaller(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_greater(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_greater_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_smaller_or_equal(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_is_nan(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_is_infinite(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_float_ceil(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_floor(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_round(*)
      typedb.float_type.new_instance
    end

    def on_raw_stdout_write(*)
      typedb.integer_type.new_instance
    end

    def on_raw_get_true(*)
      typedb.true_type.new_instance
    end

    def on_raw_get_false(*)
      typedb.false_type.new_instance
    end

    def on_raw_get_nil(*)
      typedb.nil_type.new_instance
    end

    def on_raw_get_nil_prototype(*)
      typedb.nil_type
    end

    def on_raw_get_module_prototype(*)
      typedb.module_type
    end

    def on_raw_get_ffi_library_prototype(*)
      typedb.ffi_library_type
    end

    def on_raw_get_ffi_function_prototype(*)
      typedb.ffi_function_type
    end

    def on_raw_get_ffi_pointer_prototype(*)
      typedb.ffi_pointer_type
    end

    def on_raw_get_ip_socket_prototype(*)
      typedb.ip_socket_type
    end

    def on_raw_get_unix_socket_prototype(*)
      typedb.unix_socket_type
    end

    def on_raw_get_process_prototype(*)
      typedb.process_type
    end

    def on_raw_get_read_only_file_prototype(*)
      typedb.read_only_file_type
    end

    def on_raw_get_write_only_file_prototype(*)
      typedb.write_only_file_type
    end

    def on_raw_get_read_write_file_prototype(*)
      typedb.read_write_file_type
    end

    def on_raw_get_hasher_prototype(*)
      typedb.hasher_type
    end

    def on_raw_get_generator_prototype(*)
      typedb.generator_type
    end

    def on_raw_run_block(*)
      new_object_type
    end

    def on_raw_get_string_prototype(*)
      typedb.string_type
    end

    def on_raw_get_integer_prototype(*)
      typedb.integer_type
    end

    def on_raw_get_float_prototype(*)
      typedb.float_type
    end

    def on_raw_get_trait_prototype(*)
      typedb.trait_type
    end

    def on_raw_get_array_prototype(*)
      typedb.array_type
    end

    def on_raw_get_block_prototype(*)
      typedb.block_type
    end

    def optional_array_element_value(array)
      param = array.lookup_type_parameter(Config::ARRAY_TYPE_PARAMETER)
      type = array.lookup_type_parameter_instance(param) || param

      TypeSystem::Optional.wrap(type)
    end

    def on_raw_array_length(*)
      typedb.integer_type.new_instance
    end

    def on_raw_array_at(node, _)
      optional_array_element_value(node.arguments.fetch(0).type)
    end

    def on_raw_array_set(node, _)
      node.arguments.fetch(2).type
    end

    def on_raw_array_clear(*)
      TypeSystem::Never.new
    end

    def on_raw_array_remove(node, _)
      optional_array_element_value(node.arguments.fetch(0).type)
    end

    def on_raw_time_monotonic(*)
      typedb.float_type.new_instance
    end

    def on_raw_time_system(*)
      typedb.new_array_of_type(new_object_type)
    end

    def on_raw_string_to_upper(*)
      typedb.string_type.new_instance
    end

    def on_raw_string_to_lower(*)
      typedb.string_type.new_instance
    end

    def on_raw_string_to_byte_array(*)
      typedb.byte_array_type.new_instance
    end

    def on_raw_string_size(*)
      typedb.integer_type.new_instance
    end

    def on_raw_string_length(*)
      typedb.integer_type.new_instance
    end

    def on_raw_string_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_string_concat(*)
      typedb.string_type.new_instance
    end

    def on_raw_string_slice(*)
      typedb.string_type.new_instance
    end

    def on_raw_string_byte(*)
      typedb.integer_type.new_instance
    end

    def on_raw_stdin_read(*)
      typedb.integer_type.new_instance
    end

    def on_raw_stderr_write(*)
      typedb.integer_type.new_instance
    end

    def on_raw_process_spawn(node, _)
      typedb.process_type.new_instance
    end

    def on_raw_process_send_message(node, _)
      node.arguments.fetch(1).type
    end

    def on_raw_process_receive_message(node, *)
      new_object_type
    end

    def on_raw_process_current(node, _)
      typedb.process_type.new_instance
    end

    def on_raw_process_suspend_current(*)
      TypeSystem::Never.new
    end

    def on_raw_process_terminate_current(*)
      TypeSystem::Never.new
    end

    def on_raw_get_prototype(*)
      new_object_type
    end

    def on_raw_get_attribute_names(*)
      new_object_type
    end

    def on_raw_attribute_exists(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_file_flush(*)
      typedb.nil_type.new_instance
    end

    def on_raw_stdout_flush(*)
      typedb.nil_type.new_instance
    end

    def on_raw_stderr_flush(*)
      typedb.nil_type.new_instance
    end

    def on_raw_file_open(*)
      new_object_type
    end

    def on_raw_file_path(*)
      typedb.string_type.new_instance
    end

    def on_raw_file_read(*)
      typedb.integer_type.new_instance
    end

    def on_raw_file_seek(*)
      typedb.integer_type.new_instance
    end

    def on_raw_file_size(*)
      typedb.integer_type.new_instance
    end

    def on_raw_file_write(*)
      typedb.integer_type.new_instance
    end

    def on_raw_file_remove(*)
      TypeSystem::Never.new
    end

    def on_raw_file_copy(*)
      typedb.integer_type.new_instance
    end

    def on_raw_file_type(*)
      typedb.integer_type.new_instance
    end

    def on_raw_file_time(*)
      typedb.new_array_of_type(new_object_type)
    end

    def on_raw_directory_create(*)
      TypeSystem::Never.new
    end

    def on_raw_directory_remove(*)
      TypeSystem::Never.new
    end

    def on_raw_directory_list(*)
      typedb.new_array_of_type(typedb.string_type.new_instance)
    end

    def on_raw_close(*)
      typedb.nil_type.new_instance
    end

    def on_raw_process_set_blocking(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_panic(*)
      TypeSystem::Never.new
    end

    def on_raw_exit(*)
      TypeSystem::Never.new
    end

    def on_raw_platform(*)
      typedb.string_type.new_instance
    end

    def on_raw_hasher_new(*)
      typedb.hasher_type.new_instance
    end

    def on_raw_hasher_write(node, _)
      node.arguments.fetch(0).type
    end

    def on_raw_hasher_to_hash(*)
      typedb.integer_type.new_instance
    end

    def on_raw_stacktrace(*)
      tuple = typedb.new_array_of_type(new_object_type)

      typedb.new_array_of_type(tuple)
    end

    def on_raw_block_metadata(*)
      new_object_type
    end

    def on_raw_string_format_debug(*)
      typedb.string_type.new_instance
    end

    def on_raw_string_concat_array(*)
      typedb.string_type.new_instance
    end

    def on_raw_byte_array_from_array(*)
      typedb.byte_array_type.new_instance
    end

    def on_raw_byte_array_set(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_at(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_remove(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_length(*)
      typedb.integer_type.new_instance
    end

    def on_raw_byte_array_clear(*)
      TypeSystem::Never.new
    end

    def on_raw_byte_array_equals(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_byte_array_to_string(*)
      typedb.string_type.new_instance
    end

    def on_raw_get_boolean_prototype(*)
      typedb.boolean_type
    end

    def on_raw_get_byte_array_prototype(*)
      typedb.byte_array_type
    end

    def on_raw_set_object_name(*)
      typedb.string_type.new_instance
    end

    def on_raw_current_file_path(*)
      typedb.string_type.new_instance
    end

    def on_raw_env_get(*)
      typedb.string_type.new_instance
    end

    def on_raw_env_set(*)
      typedb.string_type.new_instance
    end

    def on_raw_env_remove(*)
      TypeSystem::Never.new
    end

    def on_raw_env_variables(*)
      typedb.new_array_of_type(typedb.string_type.new_instance)
    end

    def on_raw_env_home_directory(*)
      TypeSystem::Optional.new(typedb.string_type.new_instance)
    end

    def on_raw_env_temp_directory(*)
      typedb.string_type.new_instance
    end

    def on_raw_env_get_working_directory(*)
      typedb.string_type.new_instance
    end

    def on_raw_env_set_working_directory(*)
      typedb.string_type.new_instance
    end

    def on_raw_env_arguments(*)
      typedb.new_array_of_type(typedb.string_type.new_instance)
    end

    def on_raw_process_set_panic_handler(*)
      typedb.block_type.new_instance
    end

    def on_raw_process_add_defer_to_caller(*)
      TypeSystem::Block.closure(typedb.block_type, return_type: new_object_type)
    end

    def on_raw_set_default_panic_handler(*)
      TypeSystem::Block.lambda(typedb.block_type, return_type: new_object_type)
    end

    def on_raw_process_set_pinned(*)
      typedb.boolean_type.new_instance
    end

    def on_raw_process_identifier(*)
      typedb.integer_type.new_instance
    end

    def on_raw_ffi_library_open(node, _)
      typedb.ffi_library_type.new_instance
    end

    def on_raw_ffi_function_attach(node, _)
      typedb.ffi_function_type.new_instance
    end

    def on_raw_ffi_function_call(*)
      new_object_type
    end

    def on_raw_ffi_pointer_attach(node, _)
      typedb.ffi_pointer_type.new_instance
    end

    def on_raw_ffi_pointer_read(*)
      new_object_type
    end

    def on_raw_ffi_pointer_write(*)
      new_object_type
    end

    def on_raw_ffi_pointer_from_address(node, _)
      typedb.ffi_pointer_type.new_instance
    end

    def on_raw_ffi_pointer_address(*)
      typedb.integer_type.new_instance
    end

    def on_raw_ffi_type_size(*)
      typedb.integer_type.new_instance
    end

    def on_raw_ffi_type_alignment(*)
      typedb.integer_type.new_instance
    end

    def on_raw_string_to_integer(*)
      typedb.integer_type.new_instance
    end

    def on_raw_string_to_float(*)
      typedb.float_type.new_instance
    end

    def on_raw_float_to_bits(*)
      typedb.integer_type.new_instance
    end

    def on_raw_socket_create(node, _)
      new_object_type
    end

    def on_raw_socket_write(*)
      typedb.integer_type.new_instance
    end

    def on_raw_socket_read(*)
      typedb.integer_type.new_instance
    end

    def on_raw_socket_accept(node, _)
      new_object_type
    end

    def on_raw_socket_receive_from(*)
      typedb.new_array_of_type(new_object_type)
    end

    def on_raw_socket_send_to(*)
      typedb.integer_type.new_instance
    end

    def on_raw_socket_address(*)
      typedb.new_array_of_type(new_object_type)
    end

    def on_raw_socket_get_option(*)
      new_object_type
    end

    def on_raw_socket_set_option(*)
      new_object_type
    end

    def on_raw_socket_bind(*)
      TypeSystem::Never.new
    end

    def on_raw_socket_connect(*)
      TypeSystem::Never.new
    end

    def on_raw_socket_shutdown(*)
      TypeSystem::Never.new
    end

    def on_raw_socket_listen(*)
      typedb.integer_type.new_instance
    end

    def on_raw_random_number(*)
      new_object_type
    end

    def on_raw_random_range(*)
      new_object_type
    end

    def on_raw_random_bytes(*)
      typedb.byte_array_type.new_instance
    end

    def on_raw_if(node, _)
      node.arguments.fetch(1).type.new_instance
    end

    def on_raw_module_load(*)
      typedb.module_type.new_instance
    end

    def on_raw_module_get(*)
      TypeSystem::Optional.new(typedb.module_type.new_instance)
    end

    def on_raw_module_list(*)
      typedb.new_array_of_type(typedb.module_type.new_instance)
    end

    def on_raw_module_info(*)
      typedb.string_type.new_instance
    end

    def on_raw_generator_resume(*)
      TypeSystem::Never.new
    end

    def on_raw_generator_value(*)
      new_object_type
    end

    def on_raw_generator_yielded(*)
      typedb.boolean_type.new_instance
    end
  end
end
